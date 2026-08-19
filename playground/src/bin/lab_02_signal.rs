use dsp_core::mock::signal::{SineWave, SignalGenerator};
use anyhow::Result;

fn main() -> Result<()> {
    println!("🔬 DSP-Studio Playground: Signal Generation Inspector");

    let channels = 1;
    let sample_rate = 40000.0;
    let mut sine = SineWave::new(440.0, sample_rate, 1.0);
    
    let mut buffer = vec![0.0; 10];
    sine.fill_buffer(&mut buffer, channels);

    println!("Generated 10 samples of 440Hz Sine at {}Hz:", sample_rate);
    for (i, val) in buffer.iter().enumerate() {
        println!("  Sample {}: {:>8.4}", i, val);
    }

    Ok(())
}
