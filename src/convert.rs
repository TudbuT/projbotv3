use std::{
    env,
    fs::{self, File},
    io::Write,
    path::PathBuf,
    process::{self, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use gifski::{progress::NoProgress, Repeat, Settings};

const MB_1: usize = 1024 * 1024;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;
const FAST_INTERNET_SIZE: usize = MB_1 * 3;

pub async fn convert() {
    println!("encode: encoding video...");
    if fs::create_dir("vid").is_ok() {
        // We're using ffmpeg commands because ffmpeg's api is a hunk of junk
        let mut command = process::Command::new("ffmpeg")
            .args([
                "-i",
                "vid.mp4",
                "-vf",
                &format!("scale={WIDTH}:{HEIGHT},setsar=1:1,fps=fps=25"),
                "vid/%0d.png",
            ])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("encode: unable to find or run ffmpeg");
        command.wait().expect("encode: ffmpeg failed: mp4->png");
        let mut command = process::Command::new("ffmpeg")
            .args(["-i", "vid.mp4", "aud.opus"])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("encode: unable to find or run ffmpeg");
        command.wait().expect("encode: ffmpeg failed: mp4->opus");
        fs::rename("aud.opus", "aud_encoded")
            .expect("encode: unable to move aud.opus to aud_encoded");
    }
    // ffmpeg is now done converting vid.mp4 into vid/*.png

    // Create vid_encoded and encode gifs into it
    let _ = fs::create_dir("vid_encoded");
    let dir = fs::read_dir("vid")
        .expect("encode: unable to read files")
        .count();
    let running = Arc::new(Mutex::new(0));
    println!("encode: encoding gifs...");
    for n in 0..((dir as f32 / (25.0 * 5.0)).ceil() as usize) {
        *running.lock().unwrap() += 1;
        {
            let running = running.clone();
            // This thread will not interfere with tokio because it doesn't use anything async
            // and will exit before anything important is done in tokio.
            thread::spawn(move || {
                let mut image = File::create(format!("vid_encoded/{n}"))
                    .expect("encode: unable to create gif file");
                let mut buf = Vec::with_capacity(3 * 1024 * 1024);
                let (encoder, writer) = gifski::new(Settings {
                    width: Some(WIDTH),
                    height: Some(HEIGHT),
                    quality: 100,
                    fast: false,
                    repeat: Repeat::Finite(0),
                })
                .expect("unable to start encoder");
                thread::spawn(move || {
                    writer
                        .write(&mut buf, &mut NoProgress {})
                        .expect("gif writer failed");
                    if !env::var("PROJBOTV3_FAST_INTERNET")
                        .unwrap_or("".to_owned())
                        .is_empty()
                        && buf.len() < FAST_INTERNET_SIZE
                    {
                        buf.resize(FAST_INTERNET_SIZE, 0); // extend with zeroes to unify length
                    }
                    image
                        .write_all(buf.as_slice())
                        .expect("unable to write to file");
                    *running.lock().unwrap() -= 1;
                    println!("encode: encoded {n}");
                });
                // Encode frames into gif
                println!("encode: encoding {n}...");
                for (gi, i) in ((n * (25 * 5))..dir).enumerate() {
                    // n number of previously encoded gifs * 25 frames per second * 5 seconds
                    {
                        let fi = i + 1; // because ffmpeg starts counting at 1 :p

                        encoder
                            .add_frame_png_file(
                                gi,
                                PathBuf::from(format!("vid/{fi}.png")),
                                gi as f64 / 100.0 * 4.0,
                            )
                            .expect("encode: unable to encode frame to gif");
                    }
                    // We don't want to encode something that is supposed to go into the next frame
                    if i / (25 * 5) != n {
                        break;
                    }
                }
            });
        }
        while *running.lock().unwrap() >= 6 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    while *running.lock().unwrap() != 0 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!("encode: done");
}
