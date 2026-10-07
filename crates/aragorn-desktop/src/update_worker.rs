use aragorn_app::update::{UpdateCommand, UpdateEvent, UpdateSource, Updater, execute};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

/// 업데이트 입출력 명령을 백그라운드 스레드에서 실행해 UI를 막지 않는다.
pub struct UpdateWorker {
    commands: Sender<UpdateCommand>,
    events: Receiver<UpdateEvent>,
}

impl UpdateWorker {
    pub fn spawn(
        source: Arc<dyn UpdateSource>,
        updater: Arc<dyn Updater>,
        notify: Box<dyn Fn() + Send>,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel::<UpdateCommand>();
        let (event_tx, event_rx) = mpsc::channel();
        thread::Builder::new()
            .name("update-worker".into())
            .spawn(move || {
                for command in command_rx {
                    if let Some(event) = execute(&command, source.as_ref(), updater.as_ref()) {
                        if event_tx.send(event).is_err() {
                            break;
                        }
                        notify();
                    }
                }
            })
            .expect("업데이트 워커 스레드 생성");
        Self {
            commands: command_tx,
            events: event_rx,
        }
    }

    pub fn send(&self, command: UpdateCommand) {
        let _ = self.commands.send(command);
    }

    pub fn try_recv(&self) -> Option<UpdateEvent> {
        self.events.try_recv().ok()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Option<UpdateEvent> {
        self.events.recv_timeout(timeout).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aragorn_app::update::{PackageSpec, UpdateError, UpdatePolicy};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct SlowSource;

    impl UpdateSource for SlowSource {
        fn fetch_policy(&self) -> Result<UpdatePolicy, UpdateError> {
            thread::sleep(Duration::from_millis(100));
            Err(UpdateError::Network("offline".into()))
        }
    }

    struct NoUpdater;

    impl Updater for NoUpdater {
        fn download(&self, _: &PackageSpec) -> Result<(), UpdateError> {
            Err(UpdateError::NotInstalled)
        }
        fn apply_and_restart(&self) -> Result<(), UpdateError> {
            Err(UpdateError::NotInstalled)
        }
        fn apply_on_exit(&self) -> Result<(), UpdateError> {
            Err(UpdateError::NotInstalled)
        }
    }

    #[test]
    fn worker_returns_events_asynchronously() {
        let notified = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&notified);
        let worker = UpdateWorker::spawn(
            Arc::new(SlowSource),
            Arc::new(NoUpdater),
            Box::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        );

        worker.send(UpdateCommand::FetchPolicy);
        assert_eq!(
            worker.try_recv(),
            None,
            "send는 결과를 기다리지 않아야 한다"
        );

        let event = worker.recv_timeout(Duration::from_secs(2));
        assert_eq!(
            event,
            Some(UpdateEvent::PolicyFetched(Err(UpdateError::Network(
                "offline".into()
            ))))
        );
        // 워커는 이벤트를 보낸 뒤에 알리므로, 알림이 도착할 때까지 잠깐 기다린다.
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while notified.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(notified.load(Ordering::SeqCst), 1);
    }
}
