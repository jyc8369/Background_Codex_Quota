#[derive(Debug, Default)]
pub struct ShutdownSignal {
    requested: std::sync::atomic::AtomicBool,
    notify: tokio::sync::Notify,
}

impl ShutdownSignal {
    pub fn request(&self) {
        if !self
            .requested
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            self.notify.notify_one();
        }
    }

    pub fn is_requested(&self) -> bool {
        self.requested.load(std::sync::atomic::Ordering::Acquire)
    }

    pub async fn wait(&self) {
        while !self.is_requested() {
            self.notify.notified().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::ShutdownSignal;

    #[tokio::test]
    async fn wait_completes_when_requested_concurrently() {
        for _ in 0..100 {
            let signal = Arc::new(ShutdownSignal::default());
            let waiter_signal = Arc::clone(&signal);
            let waiter = tokio::spawn(async move { waiter_signal.wait().await });
            tokio::task::yield_now().await;
            signal.request();
            tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
                .await
                .unwrap()
                .unwrap();
        }
    }
}
