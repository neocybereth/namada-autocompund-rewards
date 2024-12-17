pub fn mean(vec: &[f64]) -> Option<f64> {
    if vec.is_empty() {
        return None;
    }
    let sum: f64 = vec.iter().sum();
    Some(sum / vec.len() as f64)
}
