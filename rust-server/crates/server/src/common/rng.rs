pub trait RandomSource: Send {
    fn next_u32(&mut self) -> u32;
}

#[derive(Debug, Clone, Copy)]
pub struct SequenceRng {
    next: u32,
}

impl SequenceRng {
    pub const fn new(next: u32) -> Self {
        Self { next }
    }
}

impl RandomSource for SequenceRng {
    fn next_u32(&mut self) -> u32 {
        let value = self.next;
        self.next = self.next.wrapping_add(1);
        value
    }
}
