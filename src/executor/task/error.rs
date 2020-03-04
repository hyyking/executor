use std::any::Any;
use std::fmt;
use std::io;
use std::sync::Mutex;

pub struct JoinError {
    repr: Repr,
}

enum Repr {
    Cancelled,
    Panic(Mutex<Box<dyn Any + Send + 'static>>),
}

impl JoinError {
    pub fn cancelled() -> JoinError {
        JoinError {
            repr: Repr::Cancelled,
        }
    }
    pub fn panic(err: Box<dyn Any + Send + 'static>) -> JoinError {
        JoinError {
            repr: Repr::Panic(Mutex::new(err)),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        match &self.repr {
            Repr::Cancelled => true,
            _ => false,
        }
    }

    pub fn is_panic(&self) -> bool {
        match &self.repr {
            Repr::Panic(_) => true,
            _ => false,
        }
    }

    pub fn into_panic(self) -> Box<dyn Any + Send + 'static> {
        self.try_into_panic()
            .expect("`JoinError` reason is not a panic.")
    }
    pub fn try_into_panic(self) -> Result<Box<dyn Any + Send + 'static>, JoinError> {
        match self.repr {
            Repr::Panic(p) => Ok(p.into_inner().expect("Extracting panic from mutex")),
            _ => Err(self),
        }
    }
}

impl fmt::Display for JoinError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.repr {
            Repr::Cancelled => write!(fmt, "cancelled"),
            Repr::Panic(_) => write!(fmt, "panic"),
        }
    }
}

impl fmt::Debug for JoinError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.repr {
            Repr::Cancelled => write!(fmt, "JoinError::Cancelled"),
            Repr::Panic(_) => write!(fmt, "JoinError::Panic(...)"),
        }
    }
}

impl std::error::Error for JoinError {}

impl From<JoinError> for io::Error {
    fn from(src: JoinError) -> io::Error {
        io::Error::new(
            io::ErrorKind::Other,
            match src.repr {
                Repr::Cancelled => "task was cancelled",
                Repr::Panic(_) => "task panicked",
            },
        )
    }
}
