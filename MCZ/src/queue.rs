use std::collections::VecDeque;

/// Provider neutral track queue. Each adapter decides how to refill it.
pub struct Queue<T> {
    pub tracks: VecDeque<T>,
}

impl<T> Queue<T> {
    pub fn new() -> Self {
        Self {
            tracks: VecDeque::new(),
        }
    }
    pub fn next(&mut self) -> Option<T> {
        self.tracks.pop_front()
    }
    pub fn extend(&mut self, tracks: impl IntoIterator<Item = T>) {
        self.tracks.extend(tracks);
    }
    pub fn clear(&mut self) {
        self.tracks.clear();
    }
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self::new()
    }
}
