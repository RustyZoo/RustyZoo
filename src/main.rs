use std::error::Error;
use std::process::ExitCode;

use rustyzoo::Framebuffer;
use sdl3::audio::AudioSpec;
use sdl3::pixels::PixelFormat;

const USAGE: &str = "\
usage: rustyzoo window-smoke

  window-smoke   open a window and the default audio device, present one frame, exit";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    match (args.next(), args.next()) {
        (Some(command), None) if command == "window-smoke" => match window_smoke() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("rustyzoo: {err}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn window_smoke() -> Result<(), Box<dyn Error>> {
    let mut framebuffer = Framebuffer::new(800, 600);
    framebuffer.fill_test_pattern();

    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let audio = sdl.audio()?;

    let window = video
        .window("RustyZoo", framebuffer.width, framebuffer.height)
        .build()?;
    let mut canvas = sdl3::render::create_renderer(window, None)?;
    let textures = canvas.texture_creator();
    let mut texture = textures.create_texture_streaming(
        PixelFormat::RGBA32,
        framebuffer.width,
        framebuffer.height,
    )?;
    texture.update(None, &framebuffer.pixels, framebuffer.width as usize * 4)?;
    canvas.copy(&texture, None, None)?;
    if !canvas.present() {
        return Err(sdl3::get_error().into());
    }

    sdl.event_pump()?.pump_events();
    let _device = audio.open_playback_device(&AudioSpec::default())?;

    println!("SDL version:  {}", sdl3::version::version());
    println!("video driver: {}", video.current_video_driver());
    println!("audio driver: {}", audio.current_audio_driver());
    Ok(())
}
