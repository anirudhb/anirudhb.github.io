// TODO: actually figure out webp thread safety

use std::ops::{Deref, DerefMut};

use webp::{Encoder, WebPMemory};

pub struct ThreadSafeWebPMemory(WebPMemory);

unsafe impl Send for ThreadSafeWebPMemory {}
unsafe impl Sync for ThreadSafeWebPMemory {}

impl Deref for ThreadSafeWebPMemory {
    type Target = WebPMemory;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ThreadSafeWebPMemory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub trait ThreadSafeWebP {
    fn threadsafe_encode(&self, quality: f32) -> ThreadSafeWebPMemory;
}

impl<'a> ThreadSafeWebP for Encoder<'a> {
    fn threadsafe_encode(&self, quality: f32) -> ThreadSafeWebPMemory {
        ThreadSafeWebPMemory(self.encode(quality))
    }
}
