mod thread;

pub use thread::Parker;

pub trait Park {
    type Handle: Unpark;
    fn handle(&self) -> Self::Handle;
    fn park(&mut self) -> Result<(), std::io::Error>;
    fn park_timeout(&mut self, dur: std::time::Duration) -> Result<(), std::io::Error>;
}

pub trait Unpark: Sync + Send + 'static {
    fn unpark(&self);
}

impl<T: Unpark> Unpark for Box<T> {
    fn unpark(&self) {
        (**self).unpark()
    }
}

impl<T: Unpark> Unpark for std::sync::Arc<T> {
    fn unpark(&self) {
        (**self).unpark()
    }
}
