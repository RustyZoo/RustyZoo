//! Sound playback on SDL3: WAV members of the game's files, the menu music.
//!
//! Only the program uses this module, because the core crate does not link to SDL.

use std::error::Error;

use rustyzoo::config::{Config, to_int};
use rustyzoo::vfs::Vfs;
use sdl3::audio::{AudioDevice, AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::iostream::IOStream;

/// The most a sound may be attenuated for it to start (sound specification 4.2).
const START_LIMIT: i32 = 6000;

/// The gain for the sum `S = G + B + F + X` of a sound's attenuation terms, in hundredths of a
/// decibel; positive is quieter: `10^(-S/2000)`. A negative sum is replaced by 1 (-0.01 dB): the
/// terms never make a sound louder than full level. The one place that turns attenuation into a
/// volume (sound specification 4.1).
pub fn gain(sum: i32) -> f32 {
    10f32.powf(-(sum.clamp(1, 10_000) as f32) / 2000.0)
}

/// The global term `G` for the `[UI] userAttenuation` setting `u`: the slider position
/// `s = clamp(100 - u div 15, 0, 100)` gives `(100 - s) * 15`, or 5000 at 0 (specification 4.3).
pub fn user_term(u: i32) -> i32 {
    let slider = (100 - u / 15).clamp(0, 100);
    if slider == 0 {
        5000
    } else {
        (100 - slider) * 15
    }
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
    ///
    /// SDL is called directly: the safe wrapper of the `sdl3` crate builds a slice from a null
    /// pointer for an empty sound, which panics in debug builds.
    pub fn decode(bytes: &[u8]) -> Result<Sound, Box<dyn Error>> {
        use sdl3::sys::audio as sys;
        let source = IOStream::from_bytes(bytes)?;
        let mut spec = std::mem::MaybeUninit::<sys::SDL_AudioSpec>::uninit();
        let mut buffer = std::ptr::null_mut();
        let mut length = 0u32;
        // SAFETY: the stream outlives the call and is not closed by it; on success SDL fills
        // `spec` and hands over `length` bytes at `buffer`, which are copied and freed below.
        let loaded = unsafe {
            sys::SDL_LoadWAV_IO(
                source.raw(),
                false,
                spec.as_mut_ptr(),
                &mut buffer,
                &mut length,
            )
        };
        if !loaded {
            return Err(sdl3::get_error().into());
        }
        // SAFETY: `spec` is filled on success.
        let spec = unsafe { spec.assume_init() };
        let data = if buffer.is_null() {
            Vec::new()
        } else {
            // SAFETY: SDL allocated `length` bytes at `buffer`.
            unsafe { std::slice::from_raw_parts(buffer, length as usize) }.to_vec()
        };
        // SAFETY: the buffer came from SDL and is not used again.
        unsafe { sdl3::sys::stdinc::SDL_free(buffer.cast()) };
        let format = match spec.format {
            sys::SDL_AUDIO_U8 => AudioFormat::U8,
            sys::SDL_AUDIO_S8 => AudioFormat::S8,
            sys::SDL_AUDIO_S16LE => AudioFormat::S16LE,
            sys::SDL_AUDIO_S16BE => AudioFormat::S16BE,
            sys::SDL_AUDIO_S32LE => AudioFormat::S32LE,
            sys::SDL_AUDIO_S32BE => AudioFormat::S32BE,
            sys::SDL_AUDIO_F32LE => AudioFormat::F32LE,
            sys::SDL_AUDIO_F32BE => AudioFormat::F32BE,
            other => return Err(format!("unsupported sample format {}", other.0).into()),
        };
        let channels =
            u8::try_from(spec.channels).map_err(|_| format!("{} channels", spec.channels))?;
        Ok(Sound {
            freq: spec.freq,
            channels,
            format,
            data,
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

    /// Starts the sound at the volume of the attenuation sum `sum` (see [`gain`]); `None` if the
    /// sum is over 6000: such a sound does not start. With `looping` it starts again when it
    /// ends, as long as [`Voice::update`] is called at least once a second. A voice that is
    /// `paused` waits for [`Voice::resume`].
    pub fn play(
        &self,
        sound: Sound,
        sum: i32,
        looping: bool,
        paused: bool,
    ) -> Result<Option<Voice>, Box<dyn Error>> {
        if sum > START_LIMIT {
            return Ok(None);
        }
        let stream = self.subsystem.new_playback_stream(&sound.spec(), None)?;
        self.device.bind_stream(&stream)?;
        stream.set_gain(gain(sum))?;
        if paused {
            stream.pause()?;
        }
        stream.put_data(&sound.data)?;
        Ok(Some(Voice {
            stream,
            sound,
            looping,
        }))
    }

    /// The menu music, played with `S = G + B` (`F` and `X` are 0): `G` from
    /// `[UI] userAttenuation`, `B` from `menuMusicAttenuation` (0 when absent). The file is
    /// `menuMusic`, `sounds/mainmenu.wav` when absent. `None` if `noMenuMusic` is exactly 1 or
    /// the sum is over 6000. Fails if the music cannot be read; the game treats that as silence.
    pub fn menu_music(
        &self,
        vfs: &Vfs,
        looping: bool,
        paused: bool,
    ) -> Result<Option<(String, Voice)>, Box<dyn Error>> {
        let ini = Config::parse(vfs.ini()).ok();
        let ui = ini.as_ref().and_then(|ini| ini.section("UI"));
        let get = |key: &str| ui.and_then(|ui| ui.get(key));
        if get("noMenuMusic").and_then(to_int) == Some(1) {
            return Ok(None);
        }
        let name = get("menuMusic").map_or_else(
            || "sounds/mainmenu.wav".to_string(),
            |name| String::from_utf8_lossy(name).into_owned(),
        );
        let global = user_term(get("userAttenuation").and_then(to_int).unwrap_or(0));
        let base = get("menuMusicAttenuation").and_then(to_int).unwrap_or(0);
        let sound = Sound::decode(&vfs.read(&name)?).map_err(|e| format!("{name}: {e}"))?;
        let voice = self.play(sound, global.saturating_add(base), looping, paused)?;
        Ok(voice.map(|voice| (name, voice)))
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
    fn positive_attenuation_is_quieter_and_the_sum_is_held_to_the_range() {
        assert_eq!(gain(0), gain(1));
        assert!((gain(-1000) - 0.999).abs() < 1e-3);
        assert!((gain(1000) - 0.316_227_77).abs() < 1e-6);
        assert!((gain(2000) - 0.1).abs() < 1e-6);
        assert_eq!(gain(i32::MAX), gain(10_000));
        assert_eq!(gain(i32::MIN), gain(1));
    }

    #[test]
    fn the_user_term_is_the_requantised_slider() {
        // Specification 4.3: 600 gives 600, 37 gives 30, 1500 and more give 5000.
        let terms: Vec<i32> = [0, 600, 37, 1485, 1499, 1500, 99_999, -5, i32::MIN]
            .map(user_term)
            .to_vec();
        assert_eq!(terms, [0, 600, 30, 1485, 1485, 5000, 5000, 0, 0]);
    }

    #[test]
    fn a_wav_is_decoded() {
        let sound = Sound::decode(&wav(1, 2, 22050, 16, &[0; 400])).unwrap();
        assert_eq!(sound.describe(), "S16LE 22050 Hz 2 ch");
        assert_eq!(sound.data.len(), 400);
        assert_eq!(sound.bytes_per_second(), 88200);
    }

    #[test]
    fn a_wav_without_samples_is_not_a_panic() {
        let _ = Sound::decode(&wav(1, 1, 22050, 16, &[]));
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
