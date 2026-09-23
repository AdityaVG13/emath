//! Deliberately non-terminating candidate for the timeout and
//! cancellation paths: it never writes an answer.

fn main() {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
