//! Windows waveform output. Pinned buffers remain owned until WinMM marks them
//! complete; reset/unprepare/close run before freeing any of their memory.
use super::super::VideoError;
use std::{
    ffi::c_void,
    mem::size_of,
    sync::mpsc::{Receiver, TryRecvError},
};

const RATE: u32 = 48_000;
const BUFFER_COUNT: usize = 4;
const DONE: u32 = 1;
const PREPARED: u32 = 2;

#[repr(C, packed(1))]
struct WaveFormat {
    tag: u16,
    channels: u16,
    samples_per_sec: u32,
    bytes_per_sec: u32,
    block_align: u16,
    bits: u16,
    extra_size: u16,
}
#[repr(C)]
struct WaveHeader {
    data: *mut u8,
    length: u32,
    recorded: u32,
    user: usize,
    flags: u32,
    loops: u32,
    next: *mut WaveHeader,
    reserved: usize,
}
#[repr(C)]
struct WaveTime {
    kind: u32,
    value: u32,
    padding: [u32; 1],
}

#[link(name = "winmm")]
extern "system" {
    fn waveOutOpen(
        handle: *mut *mut c_void,
        device: u32,
        format: *const WaveFormat,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32;
    fn waveOutClose(handle: *mut c_void) -> u32;
    fn waveOutPrepareHeader(handle: *mut c_void, header: *mut WaveHeader, size: u32) -> u32;
    fn waveOutUnprepareHeader(handle: *mut c_void, header: *mut WaveHeader, size: u32) -> u32;
    fn waveOutWrite(handle: *mut c_void, header: *mut WaveHeader, size: u32) -> u32;
    fn waveOutPause(handle: *mut c_void) -> u32;
    fn waveOutRestart(handle: *mut c_void) -> u32;
    fn waveOutReset(handle: *mut c_void) -> u32;
    fn waveOutGetPosition(handle: *mut c_void, time: *mut WaveTime, size: u32) -> u32;
}
fn check(code: u32, operation: &str) -> Result<(), VideoError> {
    if code == 0 {
        Ok(())
    } else {
        Err(VideoError(format!(
            "Windows audio {operation} failed (WinMM {code})"
        )))
    }
}
struct Buffer {
    header: Box<WaveHeader>,
    samples: Vec<i16>,
    queued: bool,
}

pub(super) struct AudioOutput {
    handle: *mut c_void,
    packets: Receiver<Vec<i16>>,
    buffers: Vec<Buffer>,
    exhausted: bool,
    volume: f32,
}
impl AudioOutput {
    pub fn open(packets: Receiver<Vec<i16>>, volume: f32) -> Result<Self, VideoError> {
        let format = WaveFormat {
            tag: 1,
            channels: 2,
            samples_per_sec: RATE,
            bytes_per_sec: RATE * 4,
            block_align: 4,
            bits: 16,
            extra_size: 0,
        };
        let mut handle = std::ptr::null_mut();
        unsafe {
            check(waveOutOpen(&mut handle, u32::MAX, &format, 0, 0, 0), "open")?;
        }
        let output = Self {
            handle,
            packets,
            buffers: (0..BUFFER_COUNT)
                .map(|_| Buffer {
                    header: Box::new(WaveHeader {
                        data: std::ptr::null_mut(),
                        length: 0,
                        recorded: 0,
                        user: 0,
                        flags: 0,
                        loops: 0,
                        next: std::ptr::null_mut(),
                        reserved: 0,
                    }),
                    samples: Vec::new(),
                    queued: false,
                })
                .collect(),
            exhausted: false,
            volume,
        };
        output.pause();
        Ok(output)
    }
    pub fn play(&self) {
        unsafe {
            waveOutRestart(self.handle);
        }
    }
    pub fn pause(&self) {
        unsafe {
            waveOutPause(self.handle);
        }
    }
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
    }
    pub fn position(&self) -> f64 {
        let mut time = WaveTime {
            kind: 2,
            value: 0,
            padding: [0],
        }; // TIME_SAMPLES
        let code =
            unsafe { waveOutGetPosition(self.handle, &mut time, size_of::<WaveTime>() as u32) };
        if code != 0 {
            return 0.0;
        }
        match time.kind {
            2 => time.value as f64 / RATE as f64,
            4 => time.value as f64 / (RATE * 4) as f64, // TIME_BYTES fallback
            1 => time.value as f64 / 1000.0,            // TIME_MS fallback
            _ => 0.0,
        }
    }
    pub fn pump(&mut self) -> Result<(), VideoError> {
        for buffer in &mut self.buffers {
            // WinMM writes flags asynchronously; use a volatile read.
            let flags = unsafe { std::ptr::read_volatile(&buffer.header.flags) };
            if buffer.queued && flags & DONE == 0 {
                continue;
            }
            if flags & PREPARED != 0 {
                unsafe {
                    check(
                        waveOutUnprepareHeader(
                            self.handle,
                            &mut *buffer.header,
                            size_of::<WaveHeader>() as u32,
                        ),
                        "release buffer",
                    )?;
                }
            }
            buffer.queued = false;
            match self.packets.try_recv() {
                Ok(mut samples) => {
                    for sample in &mut samples {
                        *sample = (*sample as f32 * self.volume) as i16;
                    }
                    buffer.samples = samples;
                    buffer.header.data = buffer.samples.as_mut_ptr().cast();
                    buffer.header.length = (buffer.samples.len() * 2) as u32;
                    buffer.header.flags = 0;
                    unsafe {
                        check(
                            waveOutPrepareHeader(
                                self.handle,
                                &mut *buffer.header,
                                size_of::<WaveHeader>() as u32,
                            ),
                            "prepare buffer",
                        )?;
                        check(
                            waveOutWrite(
                                self.handle,
                                &mut *buffer.header,
                                size_of::<WaveHeader>() as u32,
                            ),
                            "queue buffer",
                        )?;
                    }
                    buffer.queued = true;
                }
                Err(TryRecvError::Empty) => (),
                Err(TryRecvError::Disconnected) => self.exhausted = true,
            }
        }
        Ok(())
    }
    pub fn ended(&self) -> bool {
        self.exhausted && self.buffers.iter().all(|b| !b.queued)
    }
}
impl Drop for AudioOutput {
    fn drop(&mut self) {
        unsafe {
            waveOutReset(self.handle);
            for buffer in &mut self.buffers {
                if std::ptr::read_volatile(&buffer.header.flags) & PREPARED != 0 {
                    waveOutUnprepareHeader(
                        self.handle,
                        &mut *buffer.header,
                        size_of::<WaveHeader>() as u32,
                    );
                }
            }
            waveOutClose(self.handle);
        }
    }
}
