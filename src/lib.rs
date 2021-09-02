#![doc = include_str!("../README.md")]

use std::{convert::Infallible, fmt::Debug, future::Future, marker::PhantomData, sync::Arc};

use parking_lot::RwLock;
use tokio::time::{sleep, Duration, Instant};

/// A value which will be refreshed asynchronously.
pub struct Refreshed<T, E> {
    inner: Arc<RwLock<RefreshState<T, E>>>,
}

impl<T, E> Clone for Refreshed<T, E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// The internal state of a [Refreshed].
struct RefreshState<T, E> {
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

impl<T, E> Refreshed<T, E> {
    /// Create an initial [Builder] value with defaults.
    pub fn builder() -> Builder<T, E> {
        Builder::default()
    }

    /// Get the most recent value
    pub fn get(&self) -> Arc<T> {
        self.inner.read().value.clone()
    }

    /// Get the timestamp of the most recent successful update
    pub fn get_updated(&self) -> Instant {
        self.inner.read().updated
    }

    /// The error message, if present, from the last attempted refresh.
    ///
    /// Note that on each successful refresh, this is reset to `None`.
    pub fn get_last_error(&self) -> Option<Arc<E>> {
        self.inner.read().last_error.clone()
    }

    #[cfg(test)]
    /// Get the full state
    fn get_state(&self) -> RefreshState<T, E> {
        self.inner.read().clone()
    }
}

/// Construct the settings around how a [Refreshed] should be created and
/// updated.
pub struct Builder<T, E> {
    duration: Duration,
    success: Arc<dyn Fn(&T) + Send + Sync>,
    error: Arc<dyn Fn(&E) + Send + Sync>,
    exit: Arc<dyn Fn() + Send + Sync>,
    _phantom: PhantomData<Result<T, E>>,
}

impl<T, E> Default for Builder<T, E> {
    fn default() -> Self {
        Builder {
            duration: Duration::from_secs(60),
            success: Arc::new(|_| ()),
            error: Arc::new(|_| ()),
            exit: Arc::new(|| log::debug!("Refresh loop exited")),
            _phantom: PhantomData,
        }
    }
}

impl<T, E> Builder<T, E>
where
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    /// Set the duration for refreshing. Default value: 60 seconds.
    pub fn duration(&mut self, duration: Duration) -> &mut Self {
        self.duration = duration;
        self
    }

    /// What should we do with error values produced while refreshing? Default: no action.
    pub fn error(&mut self, error: impl Fn(&E) + Send + Sync + 'static) -> &mut Self {
        self.error = Arc::new(error);
        self
    }

    /// What should we do with success values produced while refreshing? Default: no action.
    pub fn success(&mut self, success: impl Fn(&T) + Send + Sync + 'static) -> &mut Self {
        self.success = Arc::new(success);
        self
    }

    /// What should we do when the refresh loop exits? Default: debug level log message.
    pub fn exit(&mut self, exit: impl Fn() + Send + Sync + 'static) -> &mut Self {
        self.exit = Arc::new(exit);
        self
    }

    /// Construct a [Refreshed] value from the given initialization function, which may fail.
    ///
    /// The closure is provided `false` on the first call, and `true` on subsequent refresh calls.
    pub async fn try_build<Fut, MkFut>(&self, mut mk_fut: MkFut) -> Result<Refreshed<T, E>, E>
    where
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        MkFut: FnMut(bool) -> Fut + Send + 'static,
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
        let success = self.success.clone();
        let error = self.error.clone();
        let exit = self.exit.clone();
        tokio::spawn(async move {
            let _exit = Dropper(Some(|| exit()));
            loop {
                sleep(duration).await;
                let arc = match weak.upgrade() {
                    None => break,
                    Some(arc) => arc,
                };

                match mk_fut(true).await {
                    Err(e) => {
                        error(&e);
                        arc.write().last_error = Some(Arc::new(e));
                    }
                    Ok(t) => {
                        success(&t);
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

/// Helper type which runs the provided function when dropped.
struct Dropper<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Drop for Dropper<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f()
        }
    }
}

impl<T> Builder<T, Infallible>
where
    T: Send + Sync + 'static,
{
    /// Construct a [Refreshed] value from the given initialization function
    pub async fn build<Fut, MkFut>(&self, mut mk_fut: MkFut) -> Refreshed<T, Infallible>
    where
        Fut: Future<Output = T> + Send + 'static,
        MkFut: FnMut(bool) -> Fut + Send + 'static,
    {
        let res = self
            .try_build(move |is_refresh| {
                let fut = mk_fut(is_refresh);
                async move {
                    let t = fut.await;
                    Ok::<_, Infallible>(t)
                }
            })
            .await;

        absurd(res)
    }
}

fn absurd<T>(res: Result<T, Infallible>) -> T {
    res.expect("absurd!")
}

impl<T, E> Builder<T, E>
where
    T: Send + Sync + 'static,
    E: Debug + Send + Sync + 'static,
{
    /// Turn on default error logging when an error occurs.
    pub fn log_errors(&mut self) -> &mut Self {
        self.error(|e| log::error!("{:?}", e))
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use parking_lot::RwLock;
    use tokio::time::{sleep, Duration};

    use super::Refreshed;

    #[tokio::test]
    async fn simple_no_refresh() {
        let x = Refreshed::builder()
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
        let x = Refreshed::builder()
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
        let exited = Arc::new(RwLock::new(false));
        let exited_clone = exited.clone();
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
        let x = Refreshed::builder()
            .duration(duration)
            .exit(move || *exited_clone.write() = true)
            .try_build(mk_fut)
            .await
            .unwrap();
        assert_eq!(*x.get(), 1);
        assert_eq!(*exited.read(), false);
        sleep(duration).await;
        std::mem::drop(x);
        let val = *counter.read();
        for _ in 0..5u32 {
            sleep(duration).await;
            assert_eq!(val, *counter.read());
        }
        assert_eq!(*exited.read(), true);
    }

    #[tokio::test]
    async fn count_successes() {
        let counter = Arc::new(RwLock::new(0u32));
        let counter_clone = counter.clone();
        // start at 1, since we don't count the initial load
        let success = Arc::new(RwLock::new(1u32));
        let success_clone = success.clone();
        let mk_fut = move |_| {
            let counter_clone = counter_clone.clone();
            async move {
                let mut lock = counter_clone.write();
                *lock += 1;
                Ok::<u32, Infallible>(*lock)
            }
        };
        let duration = Duration::from_millis(10);
        let x = Refreshed::builder()
            .duration(duration)
            .success(move |_| *success_clone.write() += 1)
            .try_build(mk_fut)
            .await
            .unwrap();
        assert_eq!(*x.get(), 1);
        for _ in 0..10u32 {
            sleep(duration).await;
            assert_eq!(*x.get(), *counter.read());
            assert_eq!(*x.get(), *success.read());
        }
    }

    #[tokio::test]
    async fn simple_build() {
        let x = Refreshed::builder().build(|_| async { 42_u32 }).await;
        assert_eq!(*x.get(), 42);
    }

    #[tokio::test]
    async fn exit_on_panic() {
        let exited = Arc::new(RwLock::new(false));
        let exited_clone = exited.clone();
        let mk_fut = move |is_refresh| async move {
            if is_refresh {
                panic!("Don't panic!");
            } else {
                ()
            }
        };
        let duration = Duration::from_millis(10);
        let x = Refreshed::builder()
            .duration(duration)
            .exit(move || *exited_clone.write() = true)
            .build(mk_fut)
            .await;
        assert_eq!(*exited.read(), false);
        sleep(duration).await;
        sleep(duration).await;
        assert_eq!(*exited.read(), true);
        assert_eq!(x.get_state().last_error, None);
    }
}
