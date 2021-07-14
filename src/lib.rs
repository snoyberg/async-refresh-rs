//! # async-refresh
//!
//! See [README.md](https://github.com/snoyberg/async-refresh-rs#readme)

use std::{fmt::Debug, future::Future, sync::Arc};

use parking_lot::RwLock;
use tokio::time::{sleep, Duration, Instant};

/// A value which will be refreshed asynchronously.
pub struct Refreshed<T, E> {
    inner: Arc<RwLock<RefreshState<T, E>>>,
}

pub struct RefreshState<T, E> {
    /// The most recently updated value.
    pub value: Arc<T>,
    /// The timestamp when the most recent value was updated.
    updated: Instant,
    /// The error message, if present, from the last attempted refresh.
    last_error: Option<Arc<E>>,
}

impl<T, E> Clone for RefreshState<T, E> {
    fn clone(&self) -> Self {
        RefreshState {
            value: self.value.clone(),
            updated: self.updated,
            last_error: self.last_error.clone(),
        }
    }
}

/// Create an initial [Builder] value with defaults.
pub fn refreshed() -> Builder {
    Builder::default()
}

impl<T, E> Refreshed<T, E> {
    /// Get the most recent value
    pub fn get(&self) -> Arc<T> {
        self.inner.read().value.clone()
    }

    /// Get the full state
    pub fn get_state(&self) -> RefreshState<T, E> {
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
    ///
    /// The closure is provided `false` on the first call, and `true` on subsequent refresh calls.
    pub async fn try_build<Fut, MkFut, T, E>(&self, mut mk_fut: MkFut) -> Result<Refreshed<T, E>, E>
    where
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        MkFut: FnMut(bool) -> Fut + Send + 'static,
        T: Send + Sync + 'static,
        E: Debug + Send + Sync + 'static,
    {
        let init = RefreshState {
            value: Arc::new(mk_fut(false).await?),
            updated: Instant::now(),
            last_error: None,
        };
        let refresh = Refreshed {
            inner: Arc::new(RwLock::new(init)),
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

                match mk_fut(true).await {
                    Err(e) => {
                        log::error!("{:?}", e); // FIXME generalize
                        arc.write().last_error = Some(Arc::new(e));
                    }
                    Ok(t) => {
                        let mut lock = arc.write();
                        lock.value = Arc::new(t);
                        lock.updated = Instant::now();
                        lock.last_error = None;
                    }
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
            .try_build(|_| async { Ok::<_, Infallible>(42_u32) })
            .await
            .unwrap();
        assert_eq!(*x.get(), 42);
    }

    #[tokio::test]
    async fn refreshes() {
        let counter = Arc::new(RwLock::new(0u32));
        let counter_clone = counter.clone();
        let mk_fut = move |_| {
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
        let mk_fut = move |_| {
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
