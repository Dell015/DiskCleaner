//! `--timing`: one line per step, to standard error.

/// Prints "step: time" lines to standard error when switched on.
pub struct Stopwatch {
    on: bool,
    last: std::time::Instant,
}

impl Stopwatch {
    pub fn new(on: bool) -> Stopwatch {
        Stopwatch {
            on,
            last: std::time::Instant::now(),
        }
    }

    pub fn lap(&mut self, step: &str) {
        if self.on {
            eprintln!("timing    {step:<22} {:.2?}", self.last.elapsed());
        }
        self.last = std::time::Instant::now();
    }
}
