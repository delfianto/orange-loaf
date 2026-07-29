use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;

use crate::encoders::EncodeStep;

/// How much trailing stderr to keep for a failure message. ffmpeg's own
/// human-readable errors are near the end of its output, not the start.
const STDERR_TAIL_BYTES: usize = 4000;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeProgress {
    pub out_time_secs: f64,
    pub percent: Option<f64>,
    pub speed: Option<f64>,
    pub fps: Option<f64>,
}

/// Both the live event payload's status field *and* (constrained in
/// practice to every variant except `Running`) `run_cancelable`'s return
/// value — one enum rather than two nearly-identical ones for "the same
/// state, plus a transient in-progress marker."
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum EncodeStatus {
    Running,
    Completed,
    Failed { message: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EncodeEvent {
    job_id: String,
    status: EncodeStatus,
    progress: Option<EncodeProgress>,
}

fn emit<R: Runtime>(
    app: &AppHandle<R>,
    job_id: &str,
    status: EncodeStatus,
    progress: Option<EncodeProgress>,
) {
    // Nothing meaningful to do if the frontend isn't listening (e.g. window
    // closed mid-encode) — the job itself still runs to completion.
    let _ = app.emit(
        "encode:event",
        EncodeEvent {
            job_id: job_id.to_owned(),
            status,
            progress,
        },
    );
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEvent {
    job_id: String,
    line: String,
}

fn emit_log<R: Runtime>(app: &AppHandle<R>, job_id: &str, line: &str) {
    let _ = app.emit(
        "job:log",
        LogEvent {
            job_id: job_id.to_owned(),
            line: line.to_owned(),
        },
    );
}

/// Drains `stderr` line by line: each line is forwarded live as a `job:log`
/// event (for the Queue row's log panel, notes/plan.md §2.4) *and*
/// accumulated into a bounded tail buffer for failure diagnostics — ffmpeg's
/// actual error text (not just its exit code) is what an operator needs to
/// see (notes/plan.md §4.12).
fn spawn_stderr_drain<R: Runtime>(
    app: AppHandle<R>,
    job_id: String,
    stderr: tokio::process::ChildStderr,
) -> (tokio::task::JoinHandle<()>, Arc<Mutex<String>>) {
    let tail = Arc::new(Mutex::new(String::new()));
    let writer = Arc::clone(&tail);
    let handle = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            emit_log(&app, &job_id, &line);
            let mut buf = writer.lock().expect("stderr tail mutex poisoned");
            buf.push_str(&line);
            buf.push('\n');
            if buf.len() > STDERR_TAIL_BYTES {
                let excess = buf.len() - STDERR_TAIL_BYTES;
                buf.drain(0..excess);
            }
        }
    });
    (handle, tail)
}

/// Parses one `-progress pipe:1` `key=value` line into `pending`. Returns
/// `true` once a `progress=` line closes out a snapshot (whether or not it's
/// the final `end` one), signaling the caller should emit and reset.
fn apply_progress_line(pending: &mut EncodeProgress, duration_secs: f64, line: &str) -> bool {
    let Some((key, value)) = line.split_once('=') else {
        return false;
    };
    let value = value.trim();
    match key {
        "out_time_us" => {
            if let Ok(us) = value.parse::<f64>() {
                pending.out_time_secs = (us / 1_000_000.0).max(0.0);
                pending.percent = (duration_secs > 0.0)
                    .then(|| (pending.out_time_secs / duration_secs * 100.0).clamp(0.0, 100.0));
            }
            false
        }
        "speed" => {
            pending.speed = value.trim_end_matches('x').parse().ok();
            false
        }
        "fps" => {
            pending.fps = value.parse().ok();
            false
        }
        "progress" => true,
        _ => false,
    }
}

/// Runs an encode job's steps in order on ffmpeg, streaming progress and log
/// lines to the frontend as `encode:event`/`job:log`, without exposing any
/// way to cancel it. Used by the direct (non-queued) Convert-screen encode,
/// which has no cancel button today — [`run_cancelable`] is the real
/// implementation this delegates to, with a cancel channel whose sender is
/// simply never fired.
///
/// # Panics
///
/// Panics if the stderr-tail mutex is poisoned (i.e. the drain task itself
/// panicked, which would only happen from an allocation failure).
pub async fn run<R: Runtime>(
    app: AppHandle<R>,
    job_id: String,
    ffmpeg: PathBuf,
    steps: Vec<EncodeStep>,
    duration_secs: f64,
) -> EncodeStatus {
    let (cancel_tx, cancel_rx) = oneshot::channel();
    // Kept alive for the duration of the encode (dropping it would resolve
    // `cancel_rx` immediately, falsely "cancelling" the job) but never used.
    let _cancel_tx = cancel_tx;
    run_cancelable(app, job_id, ffmpeg, steps, duration_secs, cancel_rx).await
}

/// Same as [`run`], but selects on `cancel_rx` alongside reading ffmpeg's
/// progress output: firing the sender kills the in-flight ffmpeg process and
/// resolves this call to `EncodeStatus::Cancelled`, immediately, rather than
/// waiting for it to run to completion. Used by the queue runner, which owns
/// the paired sender per running job (see `queue::dispatch_next`).
///
/// # Panics
///
/// Panics if the stderr-tail mutex is poisoned (i.e. the drain task itself
/// panicked, which would only happen from an allocation failure).
pub async fn run_cancelable<R: Runtime>(
    app: AppHandle<R>,
    job_id: String,
    ffmpeg: PathBuf,
    steps: Vec<EncodeStep>,
    duration_secs: f64,
    cancel_rx: oneshot::Receiver<()>,
) -> EncodeStatus {
    emit(&app, &job_id, EncodeStatus::Running, None);
    tokio::pin!(cancel_rx);

    for EncodeStep::Argv(args) in steps {
        let mut child = match Command::new(&ffmpeg)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                let status = EncodeStatus::Failed {
                    message: format!("failed to start ffmpeg: {err}"),
                };
                emit(&app, &job_id, status.clone(), None);
                return status;
            }
        };

        let Some(stdout) = child.stdout.take() else {
            let status = EncodeStatus::Failed {
                message: "ffmpeg stdout was not piped".to_owned(),
            };
            emit(&app, &job_id, status.clone(), None);
            return status;
        };
        let Some(stderr) = child.stderr.take() else {
            let status = EncodeStatus::Failed {
                message: "ffmpeg stderr was not piped".to_owned(),
            };
            emit(&app, &job_id, status.clone(), None);
            return status;
        };
        let (stderr_task, stderr_tail) = spawn_stderr_drain(app.clone(), job_id.clone(), stderr);

        let mut lines = BufReader::new(stdout).lines();
        let mut pending = EncodeProgress::default();
        loop {
            tokio::select! {
                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            if apply_progress_line(&mut pending, duration_secs, &line) {
                                emit(&app, &job_id, EncodeStatus::Running, Some(pending.clone()));
                            }
                        }
                        _ => break,
                    }
                }
                _ = &mut cancel_rx => {
                    let _ = child.kill().await;
                    let _ = stderr_task.await;
                    let status = EncodeStatus::Cancelled;
                    emit(&app, &job_id, status.clone(), None);
                    return status;
                }
            }
        }

        let wait_result = child.wait().await;
        let _ = stderr_task.await;

        let success = matches!(&wait_result, Ok(status) if status.success());
        if !success {
            let tail = stderr_tail
                .lock()
                .expect("stderr tail mutex poisoned")
                .clone();
            let message = match wait_result {
                Ok(status) => format!("ffmpeg exited with {status}\n{tail}"),
                Err(err) => format!("failed to wait on ffmpeg: {err}\n{tail}"),
            };
            let status = EncodeStatus::Failed { message };
            emit(&app, &job_id, status.clone(), None);
            return status;
        }
    }

    emit(&app, &job_id, EncodeStatus::Completed, None);
    EncodeStatus::Completed
}

#[cfg(test)]
mod tests {
    use super::{EncodeProgress, apply_progress_line};

    /// A real captured block from `ffmpeg -progress pipe:1`, as verified
    /// manually against actual ffmpeg output (see notes/plan.md M3) — this
    /// pins the parser to the exact format ffmpeg really emits, not an
    /// assumed one.
    const REAL_PROGRESS_BLOCK: &[&str] = &[
        "frame=50",
        "fps=0.00",
        "stream_0_0_q=35.0",
        "bitrate=  50.1kbits/s",
        "total_size=12034",
        "out_time_us=1920000",
        "out_time_ms=1920000",
        "out_time=00:00:01.920000",
        "dup_frames=0",
        "drop_frames=0",
        "speed=64.7x",
        "progress=end",
    ];

    #[test]
    fn parses_a_real_progress_block() {
        let mut pending = EncodeProgress::default();
        let mut closed = false;
        for line in REAL_PROGRESS_BLOCK {
            closed = apply_progress_line(&mut pending, 2.0, line);
        }
        assert!(
            closed,
            "the trailing progress= line should signal a closed snapshot"
        );
        assert!((pending.out_time_secs - 1.92).abs() < 1e-9);
        assert_eq!(pending.percent, Some(96.0));
        assert_eq!(pending.speed, Some(64.7));
    }

    #[test]
    fn only_the_progress_key_closes_a_snapshot() {
        let mut pending = EncodeProgress::default();
        for key in ["out_time_us=1000000", "speed=1.0x", "fps=25"] {
            assert!(!apply_progress_line(&mut pending, 10.0, key));
        }
        assert!(apply_progress_line(&mut pending, 10.0, "progress=continue"));
    }

    #[test]
    fn percent_is_none_when_duration_is_unknown() {
        let mut pending = EncodeProgress::default();
        apply_progress_line(&mut pending, 0.0, "out_time_us=1000000");
        assert_eq!(pending.percent, None);
    }

    #[test]
    fn ignores_lines_with_no_equals_sign() {
        let mut pending = EncodeProgress::default();
        assert!(!apply_progress_line(
            &mut pending,
            10.0,
            "not a key value line"
        ));
    }
}
