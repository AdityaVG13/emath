//! Deliberately incorrect candidate: a fast sieve that also counts 1 as
//! prime. Faster than the baseline, wrong on every workload value >= 1.

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
        if k >= 1 && !composite[k] {
            count += 1;
            let mut multiple = k.max(2) * k;
            while k >= 2 && multiple <= limit {
                composite[multiple] = true;
                multiple += k;
            }
        }
        prefix[k] = count;
    }
    let counts: Vec<String> = values.iter().map(|&n| prefix[n].to_string()).collect();
    println!("{}", counts.join(" "));
}
