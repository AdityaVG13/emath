//! Baseline hot path: count primes <= n by trial division.
//! Protocol: whitespace-separated u64 values on stdin; one count per
//! value on stdout, space-separated, in input order.

use std::io::Read;

fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    let mut d = 2;
    while d * d <= n {
        if n % d == 0 {
            return false;
        }
        d += 1;
    }
    true
}

fn count_primes(n: u64) -> u64 {
    (0..=n).filter(|&k| is_prime(k)).count() as u64
}

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).expect("stdin");
    let counts: Vec<String> = input
        .split_whitespace()
        .map(|token| count_primes(token.parse().expect("u64 workload value")).to_string())
        .collect();
    println!("{}", counts.join(" "));
}
