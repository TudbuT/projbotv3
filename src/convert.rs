use std::{
    fs::{self, File},
    process::{self, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use gif::Encoder;
use png::Decoder;

pub async fn convert() {
    println!("encode: encoding video...");
    if fs::create_dir("vid").is_ok() {
        // We're using ffmpeg commands because ffmpeg's api is a hunk of junk
        let mut command = process::Command::new("ffmpeg")
            .args([
                "-i",
                "vid.mp4",
                "-vf",
                "fps=fps=25",
                "-deadline",
                "realtime",
                "vid_25fps.mp4",
            ])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("encode: unable to find or run ffmpeg");
        command.wait().expect("encode: ffmpeg failed: mp4->mp4");
        let mut command = process::Command::new("ffmpeg")
            .args([
                "-i",
                "vid_25fps.mp4",
                "-vf",
                "scale=240:180,setsar=1:1",
                "-deadline",
                "realtime",
                "vid/%0d.png",
            ])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("encode: unable to find or run ffmpeg");
        command.wait().expect("encode: ffmpeg failed: mp4->png");
        fs::remove_file("vid_25fps.mp4").expect("encode: rm vid_25fps.mp4 failed");
        let mut command = process::Command::new("ffmpeg")
            .args(["-i", "vid.mp4", "-deadline", "realtime", "aud.opus"])
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
                let mut encoder = Some(
                    Encoder::new(&mut image, 240, 180, &[]).expect("encode: unable to create gif"),
                );
                // Write the gif control bytes
                encoder
                    .as_mut()
                    .unwrap()
                    .write_extension(gif::ExtensionData::new_control_ext(
                        4,
                        gif::DisposalMethod::Any,
                        false,
                        None,
                    ))
                    .expect("encode: unable to write extension data");
                encoder
                    .as_mut()
                    .unwrap()
                    .set_repeat(gif::Repeat::Finite(0))
                    .expect("encode: unable to set repeat");
                // Encode frames into gif
                println!("encode: encoding {n}...");
                for i in (n * (25 * 5))..dir {
                    // n number of previously encoded gifs * 25 frames per second * 5 seconds
                    {
                        let i = i + 1; // because ffmpeg starts counting at 1 :p

                        // Decode frame
                        let decoder = Decoder::new(
                            File::open(format!("vid/{i}.png"))
                                .expect(format!("encode: unable to read vid/{i}.png").as_str()),
                        );
                        let mut reader = decoder.read_info().expect(
                            format!("encode: invalid ffmpeg output in vid/{i}.png").as_str(),
                        );
                        let mut buf: Vec<u8> = vec![0; reader.output_buffer_size()];
                        let info = reader.next_frame(&mut buf).expect(
                            format!("encode: invalid ffmpeg output in vid/{i}.png").as_str(),
                        );
                        let bytes = &mut buf[..info.buffer_size()];
                        // Encode frame
                        let mut frame = gif::Frame::from_rgb(240, 180, bytes);
                        // The gif crate is a little weird with extension data, it writes a
                        // block for each frame, so we have to remind it of what we want again
                        // for each frame
                        frame.delay = 4;
                        // Add to gif
                        encoder
                            .as_mut()
                            .unwrap()
                            .write_frame(&frame)
                            .expect("encode: unable to encode frame to gif");
                    }
                    // We don't want to encode something that is supposed to go into the next frame
                    if i / (25 * 5) != n {
                        break;
                    }
                }
                *running.lock().unwrap() -= 1;
                println!("encode: encoded {n}");
            });
        }
        // Always have 6 running, but no more
        while *running.lock().unwrap() >= 6 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    while *running.lock().unwrap() != 0 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!("encode: done");
}
