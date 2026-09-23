//! Candidate hot path: count primes <= n with one sieve up to max(n).
//! Same stdin/stdout protocol as the baseline.

use std::io::Read;

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).expect("stdin");
    let values: Vec<usize> = input
        .split_whitespace()
        .map(|token| token.parse().expect("usize workload value"))
        .collect();
    let limit = values.iter().copied().max().unwrap_or(0);
    let mut composite = vec![false; limit + 1];
    let mut prefix = vec![0u64; limit + 1];
    let mut count = 0;
    for k in 0..=limit {
        if k >= 2 && !composite[k] {
            count += 1;
            let mut multiple = k * k;
            while multiple <= limit {
                composite[multiple] = true;
                multiple += k;
            }
        }
        prefix[k] = count;
    }
    let counts: Vec<String> = values.iter().map(|&n| prefix[n].to_string()).collect();
    println!("{}", counts.join(" "));
}
