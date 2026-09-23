- TUI (ratatui): draw signals (they're data now — curves, LFOs, energy over the form),
  voice activity, bus meters, and the song position against the form.
- MIDI keyboard: record played notes into a phrase, with quantize/timing help.
- Stereo voices: unison spread across the field; per-voice pan.
- Audio-follower sidechain (envelope of a source's audio) next to the trigger duck.
- A better reverb (FDN) alongside the Freeverb.
- Generate patterns off the audio thread (look-ahead on the control thread) if pattern
  closures ever get heavy.
