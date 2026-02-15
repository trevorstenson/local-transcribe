/// Resample audio from one sample rate to another using linear interpolation.
/// Returns input unchanged if rates match or input is empty.
pub fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (input.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx_floor = src_idx as usize;
        let frac = src_idx - idx_floor as f64;

        let sample = if idx_floor + 1 < input.len() {
            input[idx_floor] as f64 * (1.0 - frac) + input[idx_floor + 1] as f64 * frac
        } else {
            input[idx_floor] as f64
        };

        output.push(sample as f32);
    }

    output
}
