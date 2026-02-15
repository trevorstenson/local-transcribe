/// Computes RMS amplitude levels from the tail of an audio buffer.
/// Returns `num_bars` values, each representing ~33ms of audio.
pub fn compute_levels(buffer: &[f32], sample_rate: u32, num_bars: usize) -> Vec<f32> {
    let samples_per_bar = (sample_rate as f32 * 0.033) as usize;
    let total_needed = samples_per_bar * num_bars;

    let start = if buffer.len() > total_needed {
        buffer.len() - total_needed
    } else {
        0
    };
    let relevant = &buffer[start..];

    let mut levels = Vec::with_capacity(num_bars);
    for i in 0..num_bars {
        let chunk_start = i * samples_per_bar;
        let chunk_end = ((i + 1) * samples_per_bar).min(relevant.len());
        if chunk_start >= relevant.len() {
            levels.push(0.0);
        } else {
            let chunk = &relevant[chunk_start..chunk_end];
            let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            levels.push(rms);
        }
    }
    levels
}
