//! Work on a thread of its own, with the result picked up by the UI loop.

use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

/// A job running in the background. Dropping it does not stop the thread;
/// its result is then thrown away.
#[derive(Debug)]
pub struct Task<T> {
    result: Arc<Mutex<Option<T>>>,
}

impl<T: Send + 'static> Task<T> {
    /// Run `work` on a new thread; `wake` is called when it is done (to
    /// repaint the window).
    pub fn spawn(wake: impl FnOnce() + Send + 'static, work: impl FnOnce() -> T + Send + 'static) -> Self {
        let result = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&result);
        thread::spawn(move || {
            let value = work();
            *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
            wake();
        });
        Task { result }
    }

    /// The result once the work is done (handed out once).
    pub fn take(&self) -> Option<T> {
        self.result.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

/// Run `work` in the background, repainting `ctx` when it is done.
pub fn spawn_with_repaint<T: Send + 'static>(
    ctx: &eframe::egui::Context,
    work: impl FnOnce() -> T + Send + 'static,
) -> Task<T> {
    let ctx = ctx.clone();
    Task::spawn(move || ctx.request_repaint(), work)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn hands_out_the_result_once() {
        let (sender, receiver) = mpsc::channel();
        let task = Task::spawn(move || sender.send(()).unwrap(), || 6 * 7);
        receiver.recv().unwrap();
        assert_eq!(task.take(), Some(42));
        assert_eq!(task.take(), None);
    }
}
