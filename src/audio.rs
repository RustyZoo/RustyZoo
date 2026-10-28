//! Sound playback on SDL3: WAV members of the game's files, the menu music.
//!
//! Only the program uses this module, because the core crate does not link to SDL.

use std::cell::Cell;
use std::error::Error;
use std::ffi::{c_int, c_void};
use std::sync::Arc;

use rustyzoo::music::{Music, START_LIMIT, gain};
use rustyzoo::vfs::Vfs;
use sdl3::audio::{AudioDevice, AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::iostream::IOStream;
use sdl3::sys::audio::{SDL_AudioStream, SDL_PutAudioStreamData};

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

    /// The sound with the pan of a positional start (sound specification 3): a positive `pan`
    /// lowers the left channel by that many hundredths of a decibel, a negative one the right
    /// channel. The samples become stereo 16 bit, also for a pan of 0; a format that is not 8 or 16
    /// bit mono or stereo leaves the sound as it is.
    pub fn panned(self, pan: i32) -> Sound {
        let samples: Vec<i16> = match (self.format, self.channels) {
            (AudioFormat::U8, 1 | 2) => (self.data.iter())
                .map(|&b| (i16::from(b) - 128) << 8)
                .collect(),
            (AudioFormat::S16LE, 1 | 2) => (self.data.as_chunks::<2>().0.iter())
                .map(|&c| i16::from_le_bytes(c))
                .collect(),
            _ => return self,
        };
        let (left, right) = (
            if pan > 0 { gain(pan) } else { 1.0 },
            if pan < 0 {
                gain(pan.saturating_neg())
            } else {
                1.0
            },
        );
        let frames: Vec<[i16; 2]> = match self.channels {
            1 => samples.iter().map(|&s| [s, s]).collect(),
            _ => samples.as_chunks::<2>().0.to_vec(),
        };
        let data = (frames.iter())
            .flat_map(|[l, r]| {
                let (l, r) = (
                    (f32::from(*l) * left) as i16,
                    (f32::from(*r) * right) as i16,
                );
                [l.to_le_bytes(), r.to_le_bytes()].concat()
            })
            .collect();
        Sound {
            channels: 2,
            format: AudioFormat::S16LE,
            data,
            ..self
        }
    }

    /// The bytes of one sample for each channel.
    fn frame_size(&self) -> usize {
        usize::from(self.channels)
            * match self.format {
                AudioFormat::U8 | AudioFormat::S8 => 1,
                AudioFormat::S16LE | AudioFormat::S16BE => 2,
                _ => 4,
            }
    }

    /// The last `bytes` of the sound, rounded up to whole frames: what is left to play of a
    /// sound that has `bytes` still queued.
    fn tail(mut self, bytes: usize) -> Sound {
        let frame = self.frame_size().max(1);
        let keep = bytes.div_ceil(frame).saturating_mul(frame);
        self.data.drain(..self.data.len().saturating_sub(keep));
        self
    }

    fn spec(&self) -> AudioSpec {
        AudioSpec::new(
            Some(self.freq),
            Some(self.channels.into()),
            Some(self.format),
        )
    }
}

/// The default playback device, to which every sound is bound as its own stream.
pub struct Audio {
    subsystem: sdl3::AudioSubsystem,
    device: AudioDevice,
    /// `[advanced] use8BitSound`: [`Audio::load`] tries the `altsound/` copy first.
    pub eight_bit: bool,
}

impl Audio {
    pub fn open(audio: &sdl3::AudioSubsystem) -> Result<Audio, Box<dyn Error>> {
        Ok(Audio {
            subsystem: audio.clone(),
            device: audio.open_playback_device(&AudioSpec::default())?,
            eight_bit: false,
        })
    }

    /// Reads and decodes the sound `name`. With the 8-bit set (sound 2.3) the copy under
    /// `altsound/` comes first, and a copy that is missing or cannot be decoded gives the
    /// normal name.
    pub fn load(&self, vfs: &Vfs, name: &str) -> Result<Sound, Box<dyn Error>> {
        let decode = |name: &str| Sound::decode(&vfs.read(name)?);
        let alt = format!("altsound/{name}");
        match self.eight_bit.then(|| decode(&alt)) {
            Some(Ok(sound)) => Ok(sound),
            _ => decode(name),
        }
        .map_err(|e| format!("{name}: {e}").into())
    }

    /// Silences the whole output (or restores it) without touching the voices: they keep
    /// playing, as sounds do when the system mutes a program that has lost the focus.
    pub fn mute(&self, mute: bool) -> Result<(), Box<dyn Error>> {
        Ok(self.device.set_gain(if mute { 0.0 } else { 1.0 })?)
    }

    /// Starts the sound at the volume of the attenuation sum `sum` (see [`gain`]); `None` if the
    /// sum is over 6000: such a sound does not start. With `looping` it starts again when it
    /// ends, queued by the audio thread (see [`refill`]), so it goes on while the program does
    /// not call into SDL. A voice that is `paused` waits for [`Voice::resume`].
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
        self.start(sound, sum, looping, paused).map(Some)
    }

    /// [`Audio::play`] without the limit on the sum: the sound starts however quiet it is.
    pub fn start(
        &self,
        sound: Sound,
        sum: i32,
        looping: bool,
        paused: bool,
    ) -> Result<Voice, Box<dyn Error>> {
        let stream = self.subsystem.new_playback_stream(&sound.spec(), None)?;
        self.device.bind_stream(&stream)?;
        stream.set_gain(gain(sum))?;
        if paused {
            stream.pause()?;
        }
        stream.put_data(&sound.data)?;
        let sound = Arc::new(sound);
        if looping {
            let data = Arc::as_ptr(&sound).cast_mut().cast();
            // SAFETY: `data` points into the shared sound, which lives as long as the voice, and
            // the voice destroys the stream before it drops the sound (see [`Voice`]).
            let set = unsafe {
                sdl3::sys::audio::SDL_SetAudioStreamGetCallback(stream.raw(), Some(refill), data)
            };
            if !set {
                return Err(sdl3::get_error().into());
            }
        }
        Ok(Voice {
            stream,
            sound,
            source: None,
            sum: Cell::new(sum),
            x: Cell::new(0),
            muted: Cell::new(false),
        })
    }

    /// Starts a positional sound, a one-shot or with `looping` a loop: the sound with its pan, at
    /// the volume of the sum `sum`, whose distance term is `x`; `None` if the sum is over 6000. The
    /// voice keeps the sound as it is, so that [`Voice::set_pan`] can move the pan of what is
    /// left to play (a loop starts again with the pan it began with).
    pub fn play_at(
        &self,
        sound: Sound,
        pan: i32,
        sum: i32,
        x: i32,
        looping: bool,
    ) -> Result<Option<Voice>, Box<dyn Error>> {
        if sum > START_LIMIT {
            return Ok(None);
        }
        let mut voice = self.start(sound.clone().panned(pan), sum, looping, false)?;
        voice.x.set(x);
        voice.source = Some(sound);
        Ok(Some(voice))
    }

    /// The menu music, played with its sum `S = G + B + F` (`X` is 0). `None` if the sum is over
    /// 6000. Fails if the file cannot be read; the game treats that as silence.
    pub fn menu_music(
        &self,
        vfs: &Vfs,
        music: &Music,
        looping: bool,
        paused: bool,
    ) -> Result<Option<Voice>, Box<dyn Error>> {
        let sound = self.load(vfs, music.file())?;
        self.play(sound, music.sum(), looping, paused)
    }
}

/// Called by SDL, on the audio thread, when a looping stream is short of data: queues the whole
/// sound again. A loop therefore goes on, silently if the output is muted, while the main loop
/// is blocked, and is heard where it has got to afterwards (sound 10.4, O8).
unsafe extern "C" fn refill(
    sound: *mut c_void,
    stream: *mut SDL_AudioStream,
    additional: c_int,
    _total: c_int,
) {
    // SAFETY: `sound` is the sound of the voice that owns the stream, alive until the
    // stream is destroyed; nothing writes to it.
    let sound = unsafe { &*sound.cast::<Sound>() };
    if let (true, Ok(len)) = (additional > 0, c_int::try_from(sound.data.len()))
        && len > 0
    {
        // SAFETY: SDL allows putting data from this callback; it copies `len` bytes.
        unsafe { SDL_PutAudioStreamData(stream, sound.data.as_ptr().cast(), len) };
    }
}

/// The per-play term of a sound muted by a pause, in hundredths of a decibel (sound 10.2).
const MUTE: i32 = 5000;

/// A sound being played.
pub struct Voice {
    // The stream comes first so that it is destroyed, and its callback stops, before the sound
    // that the callback reads is dropped.
    stream: AudioStreamOwner,
    sound: Arc<Sound>,
    /// The sound before its pan, for a voice that can be panned again.
    source: Option<Sound>,
    /// The attenuation sum last set, of which `x` is the per-play term (sound 4.1).
    sum: Cell<i32>,
    x: Cell<i32>,
    /// The voice is muted by a pause: its per-play term is 5000 instead of `x` (sound 10.2).
    muted: Cell<bool>,
}

impl Voice {
    pub fn pause(&self) -> Result<(), Box<dyn Error>> {
        Ok(self.stream.pause()?)
    }

    pub fn resume(&self) -> Result<(), Box<dyn Error>> {
        Ok(self.stream.resume()?)
    }

    pub fn queued_bytes(&self) -> Result<i32, Box<dyn Error>> {
        Ok(self.stream.queued_bytes()?)
    }

    /// Sets the volume to that of the attenuation sum `sum` (see [`gain`]), whose per-play term
    /// is 0. A new volume ends the muting of a pause, as the original's play does.
    pub fn set_sum(&self, sum: i32) -> Result<(), Box<dyn Error>> {
        self.set_sum_x(sum, 0)
    }

    /// [`Voice::set_sum`] for a sum whose per-play (distance) term is `x`.
    pub fn set_sum_x(&self, sum: i32, x: i32) -> Result<(), Box<dyn Error>> {
        self.sum.set(sum);
        self.x.set(x);
        self.muted.set(false);
        Ok(self.stream.set_gain(gain(sum))?)
    }

    /// Mutes the voice by 50 dB, whatever its distance, if it is playing, or gives its volume
    /// back (sound 10.2). A voice that has played to its end is left as it is.
    pub fn mute(&self, mute: bool) -> Result<(), Box<dyn Error>> {
        if mute != self.muted.get() && (!mute || self.queued_bytes()? > 0) {
            self.muted.set(mute);
            let (sum, x) = (self.sum.get(), self.x.get());
            let sum = if mute { sum - x + MUTE } else { sum };
            self.stream.set_gain(gain(sum))?;
        }
        Ok(())
    }

    /// Moves the pan of what is left to play (sound 9.1). Nothing changes for a voice that has
    /// no source or has played to its end.
    pub fn set_pan(&self, pan: i32) -> Result<(), Box<dyn Error>> {
        let Some(source) = &self.source else {
            return Ok(());
        };
        if let Ok(left @ 1..) = usize::try_from(self.queued_bytes()?) {
            let rest = source.clone().panned(pan).tail(left);
            self.stream.clear()?;
            self.stream.put_data(&rest.data)?;
        }
        Ok(())
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
    fn a_wav_is_decoded() {
        let sound = Sound::decode(&wav(1, 2, 22050, 16, &[0; 400])).unwrap();
        assert_eq!(sound.describe(), "S16LE 22050 Hz 2 ch");
        assert_eq!(sound.data.len(), 400);
    }

    #[test]
    fn a_pan_lowers_one_channel() {
        let mono = Sound::decode(&wav(1, 1, 22050, 16, &[0, 0x40, 0, 0xc0])).unwrap();
        // 20 dB less on the left: a tenth of the amplitude.
        let left = mono.clone().panned(2000);
        assert_eq!((left.channels, left.data.len()), (2, 8));
        let at = |s: &Sound, i: usize| i16::from_le_bytes([s.data[2 * i], s.data[2 * i + 1]]);
        assert_eq!((at(&left, 0), at(&left, 1)), (1638, 16384));
        assert_eq!((at(&left, 2), at(&left, 3)), (-1638, -16384));
        let right = mono.clone().panned(-2000);
        assert_eq!((at(&right, 0), at(&right, 1)), (16384, 1638));
        let centre = mono.clone().panned(0);
        assert_eq!((at(&centre, 0), at(&centre, 1)), (16384, 16384));
        // 8 bit and stereo sounds are converted, a huge pan is silence, not a panic.
        let eight = Sound::decode(&wav(1, 2, 22050, 8, &[255, 128])).unwrap();
        let p = eight.panned(i32::MIN);
        assert_eq!((p.channels, p.data.len()), (2, 4));
        assert_eq!((at(&p, 0), at(&p, 1)), (127 << 8, 0));
        let empty = Sound::decode(&wav(1, 1, 22050, 16, &[])).map(|s| s.panned(5));
        assert!(empty.is_err() || empty.is_ok_and(|s| s.data.is_empty()));
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

    #[test]
    fn the_voices_on_a_device_loop_mute_and_pan() {
        // Never a real device.
        // (the override beats a driver the environment names)
        assert!(sdl3::hint::set_with_priority(
            "SDL_AUDIO_DRIVER",
            "dummy",
            &sdl3::hint::Hint::Override
        ));
        let sdl = sdl3::init().unwrap();
        let mut audio = Audio::open(&sdl.audio().unwrap()).unwrap();
        // A quarter of a second of silence, played for 0.7 seconds without a call into the voice.
        let sound = || Sound::decode(&wav(1, 1, 22050, 16, &[0; 11025])).unwrap();
        let looping = audio.play(sound(), 0, true, false).unwrap().unwrap();
        let once = audio.play(sound(), 0, false, false).unwrap().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(700));
        // The audio thread of a loaded runner can be late: wait for each condition (at most
        // ten seconds) instead of reading the queues at one fixed instant.
        let reached = |voice: &Voice, ok: fn(i32) -> bool| {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !ok(voice.queued_bytes().unwrap()) && std::time::Instant::now() < end {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            ok(voice.queued_bytes().unwrap())
        };
        // Some bytes stay in the converter.
        assert!(reached(&once, |n| n < 100), "the device did play");
        // The one-shot has run out, so the loop was queued again by the audio thread.
        assert!(
            reached(&looping, |n| n > 1000),
            "queued by the audio thread"
        );

        // Ten seconds of silence, long enough to be playing throughout the test.
        let wave = Sound::decode(&wav(1, 1, 22050, 16, &[0; 441_000])).unwrap();
        assert!(
            audio
                .play_at(wave.clone(), 0, 6001, 0, false)
                .unwrap()
                .is_none()
        );
        let voice = audio
            .play_at(wave, 600, 1650, 1650, false)
            .unwrap()
            .unwrap();
        assert!((voice.gain().unwrap() - gain(1650)).abs() < 1e-6);
        // The distance goes, the global and base terms stay: 5000 plus 0 here.
        voice.mute(true).unwrap();
        assert!((voice.gain().unwrap() - gain(5000)).abs() < 1e-6);
        voice.mute(true).unwrap();
        assert!(
            (voice.gain().unwrap() - gain(5000)).abs() < 1e-6,
            "muted once"
        );
        voice.mute(false).unwrap();
        assert!((voice.gain().unwrap() - gain(1650)).abs() < 1e-6);
        // A new volume ends the muting.
        voice.mute(true).unwrap();
        voice.set_sum_x(1000, 400).unwrap();
        voice.mute(false).unwrap();
        assert!((voice.gain().unwrap() - gain(1000)).abs() < 1e-6);
        voice.mute(true).unwrap();
        assert!((voice.gain().unwrap() - gain(5600)).abs() < 1e-6);
        // A pan moves what is left to play: the same number of frames, never more.
        let before = voice.queued_bytes().unwrap();
        for pan in [-10_000, 0, 10_000, i32::MIN, i32::MAX] {
            voice.set_pan(pan).unwrap();
            let after = voice.queued_bytes().unwrap();
            assert!(0 < after && after <= before, "{pan}: {after} of {before}");
        }
        // A voice without a source and one that has played out ignore it.
        let plain = audio
            .play(
                Sound::decode(&wav(1, 1, 22050, 16, &[0; 4])).unwrap(),
                0,
                false,
                true,
            )
            .unwrap()
            .unwrap();
        plain.set_pan(100).unwrap();
        plain.mute(true).unwrap();

        // The 8-bit set (sound 2.3): the copy under altsound/ first, the normal name when it is
        // missing or cannot be decoded, and only the normal name without the setting.
        let dir = std::env::temp_dir().join(format!("rustyzoo-altsound-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("altsound")).unwrap();
        let write = |name: &str, bytes: &[u8]| std::fs::write(dir.join(name), bytes).unwrap();
        write("zoo.ini", b"[resource]\npath=.\n");
        write("a.wav", &wav(1, 1, 22050, 16, &[0; 8]));
        write("altsound/a.wav", &wav(1, 1, 22050, 8, &[128; 2]));
        write("b.wav", &wav(1, 1, 22050, 16, &[0; 8]));
        write("altsound/b.wav", b"not a wav");
        write("c.wav", &wav(1, 1, 22050, 16, &[0; 8]));
        let vfs = Vfs::open(&dir).unwrap();
        let formats = |audio: &Audio| {
            ["a.wav", "b.wav", "c.wav", "d.wav"]
                .map(|n| audio.load(&vfs, n).map(|s| s.describe()).ok())
        };
        let (normal, alt) = (
            "S16LE 22050 Hz 1 ch".to_string(),
            "U8 22050 Hz 1 ch".to_string(),
        );
        assert_eq!(
            formats(&audio),
            [
                Some(normal.clone()),
                Some(normal.clone()),
                Some(normal.clone()),
                None
            ]
        );
        audio.eight_bit = true;
        assert_eq!(
            formats(&audio),
            [Some(alt), Some(normal.clone()), Some(normal), None]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tail_of_a_sound_is_what_is_left_in_whole_frames() {
        let stereo = Sound::decode(&wav(1, 2, 22050, 16, &(0..32).collect::<Vec<u8>>())).unwrap();
        let tail = |n| stereo.clone().tail(n).data;
        assert_eq!(tail(12), (20..32).collect::<Vec<u8>>());
        assert_eq!(
            tail(10),
            (20..32).collect::<Vec<u8>>(),
            "rounded up to a frame"
        );
        assert!(tail(0).is_empty());
        assert_eq!(tail(usize::MAX - 1).len(), 32, "more than there is");
    }
}
