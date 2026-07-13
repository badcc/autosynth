#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Waveform {
    Sine,
    Saw,
    Square,
    Triangle,
    /// White noise. Pitch-independent; useful for percussion and texture.
    Noise,
}
