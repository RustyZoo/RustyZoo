//! Sound playback on SDL3: WAV members of the game's files, the menu music.
//!
//! Only the program uses this module, because the core crate does not link to SDL.

use std::error::Error;

use rustyzoo::config::{Config, to_flag, to_int};
use rustyzoo::vfs::Vfs;
use sdl3::audio::{AudioDevice, AudioFormat, AudioSpec, AudioSpecWAV, AudioStreamOwner};
use sdl3::iostream::IOStream;

/// The gain for an attenuation in hundredths of a decibel (0 is full volume, -10000 is silent
/// for all practical purposes): `10^(attenuation / 2000)`, the ratio of amplitudes. The one place
/// that turns the game's attenuation numbers into a volume.
pub fn gain(attenuation: i32) -> f32 {
    10f32.powf(attenuation.clamp(-10_000, 0) as f32 / 2000.0)
}

/// A decoded WAV file: samples as SDL gives them (8, 16, 32 bit integer or 32 bit float).
#[derive(Clone)]
pub struct Sound {
    pub freq: i32,
    pub channels: u8,
    pub format: AudioFormat,
    pub data: Vec<u8>,
}

impl Sound {
    /// Decodes the bytes of a WAV file; a malformed file is an error.
    pub fn decode(bytes: &[u8]) -> Result<Sound, Box<dyn Error>> {
        let mut source = IOStream::from_bytes(bytes)?;
        let wav = AudioSpecWAV::load_wav_rw(&mut source)?;
        Ok(Sound {
            freq: wav.freq,
            channels: wav.channels,
            format: wav.format,
            data: wav.buffer().to_vec(),
        })
    }

    /// `S16LE 22050 Hz 1 ch`.
    pub fn describe(&self) -> String {
        format!("{:?} {} Hz {} ch", self.format, self.freq, self.channels)
    }

    fn spec(&self) -> AudioSpec {
        AudioSpec::new(
            Some(self.freq),
            Some(self.channels.into()),
            Some(self.format),
        )
    }

    /// Bytes of one second of this sound.
    fn bytes_per_second(&self) -> usize {
        let sample = match self.format {
            AudioFormat::U8 | AudioFormat::S8 => 1,
            AudioFormat::S16LE | AudioFormat::S16BE => 2,
            _ => 4,
        };
        self.freq.max(0) as usize * usize::from(self.channels) * sample
    }
}

/// The default playback device, to which every sound is bound as its own stream.
pub struct Audio {
    subsystem: sdl3::AudioSubsystem,
    device: AudioDevice,
}

impl Audio {
    pub fn open(audio: &sdl3::AudioSubsystem) -> Result<Audio, Box<dyn Error>> {
        Ok(Audio {
            subsystem: audio.clone(),
            device: audio.open_playback_device(&AudioSpec::default())?,
        })
    }

    /// Starts the sound at the volume of `attenuation` (see [`gain`]); with `looping` it starts
    /// again when it ends, as long as [`Voice::update`] is called at least once a second. A
    /// voice that is `paused` waits for [`Voice::resume`].
    pub fn play(
        &self,
        sound: Sound,
        attenuation: i32,
        looping: bool,
        paused: bool,
    ) -> Result<Voice, Box<dyn Error>> {
        let stream = self.subsystem.new_playback_stream(&sound.spec(), None)?;
        self.device.bind_stream(&stream)?;
        stream.set_gain(gain(attenuation))?;
        if paused {
            stream.pause()?;
        }
        stream.put_data(&sound.data)?;
        Ok(Voice {
            stream,
            sound,
            looping,
        })
    }

    /// The menu music of `[UI] menuMusic`, unless `noMenuMusic` is set or it names nothing; its
    /// volume is `menuMusicAttenuation` (0 when absent). Fails if the music cannot be read.
    pub fn menu_music(
        &self,
        vfs: &Vfs,
        looping: bool,
        paused: bool,
    ) -> Result<Option<(String, Voice)>, Box<dyn Error>> {
        let Ok(ini) = Config::parse(vfs.ini()) else {
            return Ok(None);
        };
        let Some(ui) = ini.section("UI") else {
            return Ok(None);
        };
        let off = ui.get("noMenuMusic").and_then(to_flag).unwrap_or(false);
        let Some(name) = ui.get("menuMusic").filter(|name| !name.is_empty()) else {
            return Ok(None);
        };
        if off {
            return Ok(None);
        }
        let name = String::from_utf8_lossy(name).into_owned();
        let attenuation = ui.get("menuMusicAttenuation").and_then(to_int).unwrap_or(0);
        let sound = Sound::decode(&vfs.read(&name)?).map_err(|e| format!("{name}: {e}"))?;
        let voice = self.play(sound, attenuation, looping, paused)?;
        Ok(Some((name, voice)))
    }
}

/// A sound being played.
pub struct Voice {
    stream: AudioStreamOwner,
    sound: Sound,
    looping: bool,
}

impl Voice {
    pub fn pause(&self) -> Result<(), Box<dyn Error>> {
        Ok(self.stream.pause()?)
    }

    pub fn resume(&self) -> Result<(), Box<dyn Error>> {
        Ok(self.stream.resume()?)
    }

    /// Queues the sound again when a looping voice has less than one second left.
    pub fn update(&self) -> Result<(), Box<dyn Error>> {
        if self.looping && (self.queued_bytes()? as usize) < self.sound.bytes_per_second() {
            self.stream.put_data(&self.sound.data)?;
        }
        Ok(())
    }

    pub fn queued_bytes(&self) -> Result<i32, Box<dyn Error>> {
        Ok(self.stream.queued_bytes()?)
    }

    pub fn gain(&self) -> Result<f32, Box<dyn Error>> {
        Ok(self.stream.get_gain()?)
    }

    pub fn sound(&self) -> &Sound {
        &self.sound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV file of 16 bit PCM with the given header fields.
    fn wav(format: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
        let block = channels * bits / 8;
        let mut out = b"RIFF".to_vec();
        out.extend((36 + data.len() as u32).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(format.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * u32::from(block)).to_le_bytes());
        out.extend(block.to_le_bytes());
        out.extend(bits.to_le_bytes());
        out.extend(b"data");
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn attenuation_is_a_ratio_of_amplitudes() {
        assert_eq!(gain(0), 1.0);
        assert!((gain(-1000) - 0.316_227_77).abs() < 1e-6);
        assert!((gain(-2000) - 0.1).abs() < 1e-6);
        // Out of range values are held to the range.
        assert_eq!(gain(500), 1.0);
        assert_eq!(gain(i32::MIN), gain(-10_000));
    }

    #[test]
    fn a_wav_is_decoded() {
        let sound = Sound::decode(&wav(1, 2, 22050, 16, &[0; 400])).unwrap();
        assert_eq!(sound.describe(), "S16LE 22050 Hz 2 ch");
        assert_eq!(sound.data.len(), 400);
        assert_eq!(sound.bytes_per_second(), 88200);
    }

    #[test]
    fn malformed_wavs_are_errors() {
        let good = wav(1, 1, 22050, 16, &[0; 100]);
        assert!(Sound::decode(&[]).is_err());
        assert!(Sound::decode(b"RIFF").is_err());
        assert!(Sound::decode(&[0xff; 64]).is_err());
        // Every truncation, and a header field set to extreme values, must not panic.
        for end in 0..good.len() {
            let _ = Sound::decode(&good[..end]);
        }
        for at in 0..44 {
            for byte in [0, 1, 0x7f, 0x80, 0xff] {
                let mut bad = good.clone();
                bad[at] = byte;
                let _ = Sound::decode(&bad);
            }
        }
        for (format, channels, rate, bits) in [
            (1, 0, 22050, 16),
            (1, 1, 0, 16),
            (1, 300, 22050, 16),
            (1, 1, 22050, 7),
            (3, 1, 22050, 64),
            (0x55, 1, 22050, 0),
            (2, 1, 22050, 4),
            (0xfffe, 1, 22050, 16),
        ] {
            let _ = Sound::decode(&wav(format, channels, rate, bits, &[0; 64]));
        }
    }
}
