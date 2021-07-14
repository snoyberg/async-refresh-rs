//! # async-refresh
//!
//! See [README.md](https://github.com/snoyberg/async-refresh-rs#readme)

use std::{fmt::Debug, future::Future, sync::Arc};

use parking_lot::RwLock;
use tokio::time::{sleep, Duration};

/// A value which will be refreshed asynchronously.
pub struct Refreshed<T> {
    inner: Arc<RwLock<Arc<T>>>,
}

/// Create an initial [Builder] value with defaults.
pub fn refreshed() -> Builder {
    Builder::default()
}

impl<T> Refreshed<T> {
    /// Get the current value
    pub fn get(&self) -> Arc<T> {
        self.inner.read().clone()
    }
}

/// Construct the settings around how a [Refreshed] should be created and
/// updated.
pub struct Builder {
    duration: Duration,
    //on_error: Box<dyn FnMut()
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            duration: Duration::from_secs(60),
        }
    }
}

impl Builder {
    /// Set the duration for refreshing. Default value: 60 seconds.
    pub fn duration(&mut self, duration: Duration) -> &mut Self {
        self.duration = duration;
        self
    }

    /*
    /// What should we do with error values produced while refreshing? Default:
    /// use error level logging and the `Debug` implementation.
    pub fn
    */

    /*
    /// Construct a [Refreshed] value from the given initialization function
    pub async fn build<Fut, T>(self, fut: Fut) -> Refreshed<T>
    where
        Fut: Future<Output=T> + Clone + Send + 'static,
        T: Send + Sync + 'static,
    {

    }
    */

    /// Construct a [Refreshed] value from the given initialization function, which may fail.
    pub async fn try_build<Fut, MkFut, T, E>(&self, mut mk_fut: MkFut) -> Result<Refreshed<T>, E>
    where
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        MkFut: FnMut() -> Fut + Send + 'static,
        T: Send + Sync + 'static,
        E: Debug,
    {
        let init = mk_fut().await?;
        let refresh = Refreshed {
            inner: Arc::new(RwLock::new(Arc::new(init))),
        };
        let weak = Arc::downgrade(&refresh.inner);
        let duration = self.duration;
        tokio::spawn(async move {
            loop {
                sleep(duration).await;
                let arc = match weak.upgrade() {
                    None => {
                        log::debug!("Refresh loop exited"); // FIXME generalize
                        break;
                    }
                    Some(arc) => arc,
                };

                let fut = mk_fut(); // without this, we need a Sync bound on Fut
                match fut.await {
                    Err(e) => log::error!("{:?}", e), // FIXME generalize
                    Ok(t) => *arc.write() = Arc::new(t),
                }
            }
        });
        Ok(refresh)
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use parking_lot::RwLock;
    use tokio::time::{sleep, Duration};

    use super::refreshed;

    #[tokio::test]
    async fn simple_no_refresh() {
        let x = refreshed()
            .try_build(|| async { Ok::<_, Infallible>(42_u32) })
            .await
            .unwrap();
        assert_eq!(*x.get(), 42);
    }

    #[tokio::test]
    async fn refreshes() {
        let counter = Arc::new(RwLock::new(0u32));
        let counter_clone = counter.clone();
        let mk_fut = move || {
            let counter_clone = counter_clone.clone();
            async move {
                let mut lock = counter_clone.write();
                *lock += 1;
                Ok::<u32, Infallible>(*lock)
            }
        };
        let duration = Duration::from_millis(10);
        let x = refreshed()
            .duration(duration)
            .try_build(mk_fut)
            .await
            .unwrap();
        assert_eq!(*x.get(), 1);
        for _ in 0..10u32 {
            sleep(duration).await;
            assert_eq!(*x.get(), *counter.read());
        }
    }

    #[tokio::test]
    async fn stops_refreshing() {
        let counter = Arc::new(RwLock::new(0u32));
        let counter_clone = counter.clone();
        let mk_fut = move || {
            let counter_clone = counter_clone.clone();
            async move {
                let mut lock = counter_clone.write();
                *lock += 1;
                Ok::<u32, Infallible>(*lock)
            }
        };
        let duration = Duration::from_millis(10);
        let x = refreshed()
            .duration(duration)
            .try_build(mk_fut)
            .await
            .unwrap();
        assert_eq!(*x.get(), 1);
        sleep(duration).await;
        std::mem::drop(x);
        let val = *counter.read();
        for _ in 0..5u32 {
            sleep(duration).await;
            assert_eq!(val, *counter.read());
        }
    }
}
