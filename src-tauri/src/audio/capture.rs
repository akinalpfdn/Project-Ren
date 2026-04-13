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

    let (config, source_rate, source_channels) = build_stream_config(&device)?;
    let (tx, rx) = mpsc::channel::<AudioSamples>(64);

    let samples_per_frame = ((AUDIO_SAMPLE_RATE as u64 * buffer_duration_ms) / 1000) as usize;
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_frame * 2);

    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let converted = resample_and_downmix(data, source_rate, source_channels);
            buffer.extend_from_slice(&converted);
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
        "Audio capture started: source {}Hz {}ch -> {}Hz mono, {}ms frames",
        source_rate, source_channels, AUDIO_SAMPLE_RATE, buffer_duration_ms
    );

    Ok((stream, rx))
}

/// Picks a StreamConfig the device actually supports.
/// Returns the config plus the source sample rate and channel count so the
/// capture callback can downmix and resample to 16 kHz mono f32.
fn build_stream_config(device: &Device) -> Result<(StreamConfig, u32, u16)> {
    let supported: Vec<_> = device
        .supported_input_configs()
        .context("Could not query microphone capabilities")?
        .collect();

    // Preferred: exact match on 16 kHz mono f32.
    for range in &supported {
        if range.sample_format() == SampleFormat::F32
            && range.channels() == AUDIO_CHANNELS
            && range.min_sample_rate().0 <= AUDIO_SAMPLE_RATE
            && range.max_sample_rate().0 >= AUDIO_SAMPLE_RATE
        {
            return Ok((
                StreamConfig {
                    channels: AUDIO_CHANNELS,
                    sample_rate: cpal::SampleRate(AUDIO_SAMPLE_RATE),
                    buffer_size: cpal::BufferSize::Default,
                },
                AUDIO_SAMPLE_RATE,
                AUDIO_CHANNELS,
            ));
        }
    }

    // Fallback: accept the device's native f32 config and convert in the callback.
    let default = device
        .default_input_config()
        .context("Could not get default microphone config")?;

    if default.sample_format() != SampleFormat::F32 {
        anyhow::bail!(
            "Microphone default sample format is {:?}; only f32 is supported right now.",
            default.sample_format()
        );
    }

    let source_rate = default.sample_rate().0;
    let source_channels = default.channels();
    warn!(
        "Microphone lacks native 16 kHz mono f32. Falling back to {} Hz {} ch and converting.",
        source_rate, source_channels
    );

    Ok((
        StreamConfig {
            channels: source_channels,
            sample_rate: cpal::SampleRate(source_rate),
            buffer_size: cpal::BufferSize::Default,
        },
        source_rate,
        source_channels,
    ))
}

/// Downmixes multi-channel audio to mono (channel average) and linearly
/// resamples to 16 kHz. Adequate for speech; production-grade resampling
/// (e.g. rubato with a sinc kernel) can replace this later.
fn resample_and_downmix(input: &[f32], source_rate: u32, source_channels: u16) -> Vec<f32> {
    let ch = source_channels.max(1) as usize;
    let mono: Vec<f32> = if ch == 1 {
        input.to_vec()
    } else {
        input
            .chunks(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    };

    if source_rate == AUDIO_SAMPLE_RATE || mono.is_empty() {
        return mono;
    }

    let ratio = AUDIO_SAMPLE_RATE as f64 / source_rate as f64;
    let out_len = (mono.len() as f64 * ratio).round() as usize;
    let max_idx = mono.len() - 1;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let a = mono[idx.min(max_idx)];
        let b = mono[(idx + 1).min(max_idx)];
        out.push(a + (b - a) * frac);
    }
    out
}
