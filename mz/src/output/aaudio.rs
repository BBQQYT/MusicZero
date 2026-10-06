//! Direct Android AAudio output. All native stream calls stay on one worker:
//! no Java VM, audio daemon, subprocess, or concurrent close/write is needed.
use super::*;
use libloading::Library;
use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

type Pointer = *mut c_void;
type Setter = unsafe extern "C" fn(Pointer, i32);
type Query = unsafe extern "C" fn(Pointer) -> i32;

struct Api {
    create: unsafe extern "C" fn(*mut Pointer) -> i32,
    rate: Setter,
    channels: Setter,
    format: Setter,
    sharing: Setter,
    open: unsafe extern "C" fn(Pointer, *mut Pointer) -> i32,
    delete: Query,
    start: Query,
    stop: Query,
    close: Query,
    get_rate: Query,
    get_channels: Query,
    get_format: Query,
    burst: Query,
    buffer: unsafe extern "C" fn(Pointer, i32) -> i32,
    write: unsafe extern "C" fn(Pointer, *const c_void, i32, i64) -> i32,
    text: unsafe extern "C" fn(i32) -> *const c_char,
    _library: Library,
}

impl Api {
    fn load() -> Result<Self> {
        // AAudio is a public Android platform library since API 26. Function
        // signatures below match the NDK's aaudio/AAudio.h (opaque pointers).
        // Keeping the Library owned by Api outlives every native call.
        unsafe {
            let library = Library::new("libaaudio.so")
                .map_err(|e| format!("AAudio unavailable: {e}. Android 8 or newer is required"))?;
            macro_rules! symbol {
                ($name:literal) => {
                    *library.get(concat!($name, "\0").as_bytes())?
                };
            }
            Ok(Self {
                create: symbol!("AAudio_createStreamBuilder"),
                rate: symbol!("AAudioStreamBuilder_setSampleRate"),
                channels: symbol!("AAudioStreamBuilder_setChannelCount"),
                format: symbol!("AAudioStreamBuilder_setFormat"),
                sharing: symbol!("AAudioStreamBuilder_setSharingMode"),
                open: symbol!("AAudioStreamBuilder_openStream"),
                delete: symbol!("AAudioStreamBuilder_delete"),
                start: symbol!("AAudioStream_requestStart"),
                stop: symbol!("AAudioStream_requestStop"),
                close: symbol!("AAudioStream_close"),
                get_rate: symbol!("AAudioStream_getSampleRate"),
                get_channels: symbol!("AAudioStream_getChannelCount"),
                get_format: symbol!("AAudioStream_getFormat"),
                burst: symbol!("AAudioStream_getFramesPerBurst"),
                buffer: symbol!("AAudioStream_setBufferSizeInFrames"),
                write: symbol!("AAudioStream_write"),
                text: symbol!("AAudio_convertResultToText"),
                _library: library,
            })
        }
    }

    fn check(&self, code: i32, operation: &str) -> Result<()> {
        if code < 0 {
            // The API returns a static, NUL-terminated description for all codes.
            let text = unsafe { (self.text)(code) };
            let text = if text.is_null() {
                "unknown error".into()
            } else {
                unsafe { CStr::from_ptr(text) }.to_string_lossy()
            };
            return Err(format!("AAudio {operation}: {text} ({code})").into());
        }
        Ok(())
    }
}

struct Stream {
    pointer: Pointer,
    api: Api,
}

impl Stream {
    fn open() -> Result<Self> {
        let api = Api::load()?;
        let mut builder = std::ptr::null_mut();
        // Builders/streams are confined to this worker thread. Each successfully
        // created builder is deleted; each opened stream is closed exactly once.
        unsafe { api.check((api.create)(&mut builder), "create builder")? };
        if builder.is_null() {
            return Err("AAudio returned an empty builder".into());
        }
        let mut pointer = std::ptr::null_mut();
        let opened = unsafe {
            (api.rate)(builder, 48000);
            (api.channels)(builder, 2);
            (api.format)(builder, 2); // AAUDIO_FORMAT_PCM_FLOAT
            (api.sharing)(builder, 1); // AAUDIO_SHARING_MODE_SHARED
            let result = (api.open)(builder, &mut pointer);
            (api.delete)(builder);
            result
        };
        api.check(opened, "open stream")?;
        if pointer.is_null() {
            return Err("AAudio returned an empty stream".into());
        }
        let stream = Self { pointer, api };
        unsafe {
            if (stream.api.get_rate)(pointer) != 48000
                || (stream.api.get_channels)(pointer) != 2
                || (stream.api.get_format)(pointer) != 2
            {
                return Err("AAudio did not accept float32 stereo at 48 kHz".into());
            }
            let burst = (stream.api.burst)(pointer);
            if let Some(size) = burst.checked_mul(2).filter(|size| *size > 0) {
                // Two mixer blocks leave headroom for Termux scheduling jitter.
                stream
                    .api
                    .check((stream.api.buffer)(pointer, size.max(960)), "set buffer")?;
            }
            stream
                .api
                .check((stream.api.start)(pointer), "start stream")?;
        }
        Ok(stream)
    }

    fn write(&mut self, pcm: &[f32; 960], stop: &AtomicBool) -> Result<()> {
        let mut offset = 0;
        let mut progress = Instant::now();
        while offset < 480 && !stop.load(Ordering::Relaxed) {
            let remaining = 480 - offset;
            // Buffer stays alive until the synchronous call returns. A finite
            // timeout lets shutdown stop even when the device is not draining.
            let written = unsafe {
                (self.api.write)(
                    self.pointer,
                    pcm[offset * 2..].as_ptr().cast(),
                    remaining as i32,
                    100_000_000,
                )
            };
            self.api.check(written, "write")?;
            if written as usize > remaining {
                return Err("AAudio returned an invalid frame count".into());
            }
            if written == 0 {
                if progress.elapsed() > Duration::from_secs(2) {
                    return Err("AAudio output stalled for 2 seconds".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            } else {
                offset += written as usize;
                progress = Instant::now();
            }
        }
        Ok(())
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // No callbacks or other threads access this pointer, including shutdown.
        unsafe {
            (self.api.stop)(self.pointer);
            (self.api.close)(self.pointer);
        }
    }
}

pub struct Output {
    mixer: Mixer,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    failures: mpsc::Receiver<String>,
}

impl Output {
    pub fn open() -> Result<Self> {
        let (mixer, mut source) = rodio::mixer::mixer(2, 48000);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (ready, started) = mpsc::sync_channel(1);
        let (failed, failures) = mpsc::channel();
        let mut output = Self {
            mixer,
            stop,
            worker: None,
            failures,
        };
        output.worker = Some(std::thread::Builder::new().name("mz-aaudio".into()).spawn(
            move || {
                let mut stream = match Stream::open() {
                    Ok(stream) => stream,
                    Err(error) => {
                        let _ = ready.send(Err(error.to_string()));
                        return;
                    }
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                let frame = Duration::from_millis(10);
                let mut deadline = Instant::now();
                let mut pcm = [0.0; 960];
                while !flag.load(Ordering::Relaxed) {
                    for sample in &mut pcm {
                        *sample = source.next().unwrap_or(0.0);
                    }
                    if let Err(error) = stream.write(&pcm, &flag) {
                        let _ = failed.send(error.to_string());
                        return;
                    }
                    // Bound mixer read-ahead even if Android accepts a large buffer.
                    // Partial native writes retry this same block without skipping.
                    deadline += frame;
                    let now = Instant::now();
                    if deadline > now {
                        std::thread::sleep(deadline - now);
                    } else {
                        deadline = now;
                    }
                }
            },
        )?);
        started
            .recv()
            .map_err(|_| "AAudio worker stopped during startup")?
            .map_err(
                |error: String| -> Box<dyn std::error::Error + Send + Sync> { error.into() },
            )?;
        log::info!("Audio output: native Android AAudio (48 kHz stereo)");
        Ok(output)
    }

    pub fn mixer(&self) -> &Mixer {
        &self.mixer
    }

    pub fn check(&mut self) -> Result<()> {
        match self.failures.try_recv() {
            Ok(error) => Err(error.into()),
            Err(mpsc::TryRecvError::Disconnected) => Err("AAudio writer stopped".into()),
            Err(mpsc::TryRecvError::Empty) => Ok(()),
        }
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
