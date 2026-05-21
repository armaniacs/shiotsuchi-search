//! Quantization benchmark & precision comparison for embedding vectors.
//!
//! Compares f16 (half-precision) and binary (sign-bit) quantization
//! against the f32 ground truth using precision@k metrics.
//!
//! Run: cargo bench -p shiotsuchi-core --bench quantization
//! Requires no model file — generates random vectors on the fly.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

struct XorShift32(u32);

impl XorShift32 {
    fn new(seed: u32) -> Self {
        Self(seed)
    }

    fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }
}

fn f32_to_f16(f: f32) -> u16 {
    let bits = f.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7fffff;
    let exp = exponent - 127 + 15;
    if exp >= 31 {
        sign | 0x7c00
    } else if exp <= 0 {
        if exp < -10 {
            sign
        } else {
            let m = (mantissa | 0x800000) >> (14 - exp);
            sign | m as u16
        }
    } else {
        sign | (exp as u16) << 10 | (mantissa >> 13) as u16
    }
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exponent = ((h >> 10) & 0x1f) as u32;
    let mantissa = (h & 0x3ff) as u32;
    if exponent == 0 {
        if mantissa == 0 {
            f32::from_bits(sign)
        } else {
            f32::from_bits(sign | (127 - 15 - 10) << 23 | mantissa << 13)
        }
    } else if exponent == 31 {
        f32::from_bits(sign | 0x7f800000 | mantissa << 13)
    } else {
        f32::from_bits(sign | (exponent + 127 - 15) << 23 | mantissa << 13)
    }
}

fn quantize_f16(data: &[f32]) -> Vec<u16> {
    data.iter().map(|&v| f32_to_f16(v)).collect()
}

fn quantize_binary(data: &[f32]) -> Vec<u64> {
    data.chunks(64)
        .map(|chunk| {
            chunk.iter().enumerate().fold(0u64, |acc, (i, &v)| {
                if v > 0.0 {
                    acc | (1u64 << i)
                } else {
                    acc
                }
            })
        })
        .collect()
}

fn cosine_similarity_f32(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(&x, &y)| x as f64 * y as f64).sum();
    let na: f64 = a.iter().map(|&x| x as f64 * x as f64).sum();
    let nb: f64 = b.iter().map(|&x| x as f64 * x as f64).sum();
    dot / (na * nb).sqrt()
}

fn cosine_approximation_f16(a: &[u16], b: &[u16]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| f16_to_f32(x) as f64 * f16_to_f32(y) as f64)
        .sum();
    let na: f64 = a.iter().map(|&x| {
        let v = f16_to_f32(x) as f64;
        v * v
    }).sum();
    let nb: f64 = b.iter().map(|&x| {
        let v = f16_to_f32(x) as f64;
        v * v
    }).sum();
    dot / (na * nb).sqrt()
}

fn cosine_approximation_binary(a: &[u64], b: &[u64]) -> f64 {
    let total_bits = (a.len() * 64) as f64;
    let hamming: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (x ^ y).count_ones() as u64)
        .sum();
    1.0 - 2.0 * hamming as f64 / total_bits
}

fn generate_random_vectors(count: usize, dims: usize) -> Vec<Vec<f32>> {
    let mut rng = XorShift32::new(42);
    (0..count)
        .map(|_| (0..dims).map(|_| rng.next_f32()).collect())
        .collect()
}

fn bench_quantization_speed(c: &mut Criterion) {
    let data = generate_random_vectors(100, 1024);

    c.bench_function("quantize_f16_100_vectors", |b| {
        b.iter(|| {
            let result: Vec<Vec<u16>> = data.iter().map(|v| quantize_f16(v)).collect();
            black_box(result)
        })
    });

    c.bench_function("quantize_binary_100_vectors", |b| {
        b.iter(|| {
            let result: Vec<Vec<u64>> = data.iter().map(|v| quantize_binary(v)).collect();
            black_box(result)
        })
    });
}

fn bench_similarity_speed(c: &mut Criterion) {
    let data = generate_random_vectors(2, 1024);
    let f16_data: Vec<Vec<u16>> = data.iter().map(|v| quantize_f16(v)).collect();
    let bin_data: Vec<Vec<u64>> = data.iter().map(|v| quantize_binary(v)).collect();

    c.bench_function("cosine_similarity_f32", |b| {
        b.iter(|| black_box(cosine_similarity_f32(&data[0], &data[1])))
    });

    c.bench_function("cosine_approximation_f16", |b| {
        b.iter(|| black_box(cosine_approximation_f16(&f16_data[0], &f16_data[1])))
    });

    c.bench_function("cosine_approximation_binary", |b| {
        b.iter(|| black_box(cosine_approximation_binary(&bin_data[0], &bin_data[1])))
    });
}

fn bench_quantization_precision(c: &mut Criterion) {
    const N: usize = 200;
    const DIMS: usize = 1024;
    const Q: usize = 5;
    const KS: &[usize] = &[1, 5, 10, 50];

    let data = generate_random_vectors(N, DIMS);
    let queries: Vec<Vec<f32>> = data.iter().take(Q).cloned().collect();
    let candidates: Vec<Vec<f32>> = data.iter().skip(Q).cloned().collect();
    let f16_candidates: Vec<Vec<u16>> = candidates.iter().map(|v| quantize_f16(v)).collect();
    let bin_candidates: Vec<Vec<u64>> = candidates.iter().map(|v| quantize_binary(v)).collect();
    let f16_queries: Vec<Vec<u16>> = queries.iter().map(|q| quantize_f16(q)).collect();
    let bin_queries: Vec<Vec<u64>> = queries.iter().map(|q| quantize_binary(q)).collect();

    let mut f32_results: Vec<Vec<(usize, f64)>> = Vec::new();
    for q in &queries {
        let mut scores: Vec<(usize, f64)> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (i, cosine_similarity_f32(q, c)))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        f32_results.push(scores);
    }

    let mut f16_results: Vec<Vec<(usize, f64)>> = Vec::new();
    for q in &f16_queries {
        let mut scores: Vec<(usize, f64)> = f16_candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (i, cosine_approximation_f16(q, c)))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        f16_results.push(scores);
    }

    let mut bin_results: Vec<Vec<(usize, f64)>> = Vec::new();
    for q in &bin_queries {
        let mut scores: Vec<(usize, f64)> = bin_candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (i, cosine_approximation_binary(q, c)))
            .collect();
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        bin_results.push(scores);
    }

    eprintln!("\n--- Quantization Precision@k ---");
    for &k in KS {
        let mut f16_total = 0.0;
        let mut bin_total = 0.0;
        for q_idx in 0..Q {
            let gt: std::collections::HashSet<usize> =
                f32_results[q_idx].iter().take(k).map(|(i, _)| *i).collect();
            let f16_top: std::collections::HashSet<usize> =
                f16_results[q_idx].iter().take(k).map(|(i, _)| *i).collect();
            let bin_top: std::collections::HashSet<usize> =
                bin_results[q_idx].iter().take(k).map(|(i, _)| *i).collect();
            f16_total += gt.intersection(&f16_top).count() as f64 / k as f64;
            bin_total += gt.intersection(&bin_top).count() as f64 / k as f64;
        }
        let f16_precision = f16_total / Q as f64;
        let bin_precision = bin_total / Q as f64;
        eprintln!("  precision@k={}: f16={:.4} binary={:.4}", k, f16_precision, bin_precision);
    }
    eprintln!("---\n");

    c.bench_function("rerank_f32_195_candidates", |b| {
        let q = &queries[0];
        b.iter(|| {
            let mut scores: Vec<(usize, f64)> = candidates
                .iter()
                .enumerate()
                .map(|(i, c)| (i, cosine_similarity_f32(q, c)))
                .collect();
            scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            black_box(scores)
        })
    });

    c.bench_function("rerank_f16_195_candidates", |b| {
        let q = &f16_queries[0];
        b.iter(|| {
            let mut scores: Vec<(usize, f64)> = f16_candidates
                .iter()
                .enumerate()
                .map(|(i, c)| (i, cosine_approximation_f16(q, c)))
                .collect();
            scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            black_box(scores)
        })
    });

    c.bench_function("rerank_binary_195_candidates", |b| {
        let q = &bin_queries[0];
        b.iter(|| {
            let mut scores: Vec<(usize, f64)> = bin_candidates
                .iter()
                .enumerate()
                .map(|(i, c)| (i, cosine_approximation_binary(q, c)))
                .collect();
            scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            black_box(scores)
        })
    });
}

criterion_group!(quant_benches, bench_quantization_speed, bench_similarity_speed, bench_quantization_precision);
criterion_main!(quant_benches);
