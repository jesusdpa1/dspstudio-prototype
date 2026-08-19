use dsp_core::mock::signal::{SineWave, WhiteNoise, SignalGenerator};

#[test]
fn test_sine_wave_determinism() {
    let mut sine = SineWave::new(440.0, 44100.0, 1.0);
    let mut buf1 = vec![0.0; 100];
    let mut buf2 = vec![0.0; 100];
    
    sine.fill_buffer(&mut buf1, 1);
    // Resetting/Recreating to check same output
    let mut sine2 = SineWave::new(440.0, 44100.0, 1.0);
    sine2.fill_buffer(&mut buf2, 1);
    
    assert_eq!(buf1, buf2);
}

#[test]
fn test_white_noise_reproducibility() {
    let mut noise1 = WhiteNoise::new(12345, 1.0);
    let mut noise2 = WhiteNoise::new(12345, 1.0);
    let mut buf1 = vec![0.0; 1000];
    let mut buf2 = vec![0.0; 1000];
    
    noise1.fill_buffer(&mut buf1, 1);
    noise2.fill_buffer(&mut buf2, 1);
    
    assert_eq!(buf1, buf2);
}
