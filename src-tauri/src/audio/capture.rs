use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::defaults::{AUDIO_CHANNELS, AUDIO_SAMPLE_RATE};

/// Raw PCM audio samples (f32, 16kHz mono).
pub type AudioSamples = Vec<f32>;

/// Captures audio from the default microphone and sends raw PCM frames
/// down a channel. Returns the stream handle (must be kept alive) and
/// the receiver end of the channel.
pub fn start_capture(
    buffer_duration_ms: u64,
) -> Result<(Stream, mpsc::Receiver<AudioSamples>)> {
    let host = cpal::default_host();

    let device = host
        .default_input_device()
        .context("No microphone found. Check Windows Settings > Privacy > Microphone.")?;

    info!("Microphone: {}", device.name().unwrap_or_default());

    let config = build_stream_config(&device)?;
    let (tx, rx) = mpsc::channel::<AudioSamples>(64);

    let samples_per_frame = ((AUDIO_SAMPLE_RATE as u64 * buffer_duration_ms) / 1000) as usize;
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_frame * 2);

    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            buffer.extend_from_slice(data);
            while buffer.len() >= samples_per_frame {
                let frame: AudioSamples = buffer.drain(..samples_per_frame).collect();
                if let Err(e) = tx.try_send(frame) {
                    warn!("Audio buffer overflow (consumer too slow): {}", e);
                }
            }
        },
        move |err| {
            error!("Audio capture error: {}", err);
        },
        None,
    )
    .context("Failed to build audio input stream")?;

    stream.play().context("Failed to start audio stream")?;
    info!(
        "Audio capture started: {}Hz mono, {}ms frames",
        AUDIO_SAMPLE_RATE, buffer_duration_ms
    );

    Ok((stream, rx))
}

/// Builds a StreamConfig targeting 16kHz mono f32.
/// Falls back gracefully if the device doesn't support f32 natively.
fn build_stream_config(device: &Device) -> Result<StreamConfig> {
    let supported = device
        .supported_input_configs()
        .context("Could not query microphone capabilities")?;

    // Prefer f32, 16kHz, mono
    for range in supported {
        if range.sample_format() == SampleFormat::F32
            && range.channels() == AUDIO_CHANNELS
            && range.min_sample_rate().0 <= AUDIO_SAMPLE_RATE
            && range.max_sample_rate().0 >= AUDIO_SAMPLE_RATE
        {
            return Ok(StreamConfig {
                channels: AUDIO_CHANNELS,
                sample_rate: cpal::SampleRate(AUDIO_SAMPLE_RATE),
                buffer_size: cpal::BufferSize::Default,
            });
        }
    }

    // Fallback: use default config and resample in future if needed
    warn!(
        "Microphone does not natively support 16kHz mono f32. \
         Using default config — transcription quality may be affected."
    );
    let default = device
        .default_input_config()
        .context("Could not get default microphone config")?;

    Ok(StreamConfig {
        channels: default.channels().min(AUDIO_CHANNELS),
        sample_rate: cpal::SampleRate(AUDIO_SAMPLE_RATE),
        buffer_size: cpal::BufferSize::Default,
    })
}
