// SPDX-License-Identifier: GPL-3.0-only
use crate::{Commands, Device, Error, Recording, Status, Submitted};
use ash::vk;
use std::sync::Arc;
use std::time::Duration;

/// Measures the GPU time between two points in the work submitted to the queue. [`GpuTimer::start`] returns a
/// [`RunningTimer`], [`RunningTimer::stop`] returns a [`StoppedTimer`], and [`StoppedTimer::reset`] returns a
/// `GpuTimer` that can start again.
pub struct GpuTimer {
    start: Commands,
    stop: Commands,
    clock: Clock,
}

pub struct RunningTimer {
    start: Submitted,
    stop: Commands,
    clock: Clock,
}

pub struct StoppedTimer {
    start: Submitted,
    stop: Submitted,
    clock: Clock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Pending,
    Elapsed(Duration),
}

/// The pool of the start and stop timestamp queries, the timestamp period in nanoseconds per tick and the mask of the
/// valid timestamp bits. Each timer declares the clock after its commands, so the clock drops after the submissions
/// that write the queries.
struct Clock {
    queries: QueryPool,
    period: f64,
    mask: u64,
}

struct QueryPool {
    device: Arc<Device>,
    handle: vk::QueryPool,
}

const START: u32 = 0;
const STOP: u32 = 1;
const QUERIES: u32 = 2;

impl GpuTimer {
    /// # Errors
    /// Fails when the queue family records no timestamps, or the driver cannot create the queries or commands.
    pub fn new(device: &Arc<Device>) -> Result<Self, Error> {
        let physical = device.physical_device();
        let families = physical.queue_families();
        let family = families
            .get(device.queue_family() as usize)
            .ok_or(Error::QueueFamily(device.queue_family()))?;
        let unused_bits = u64::BITS.checked_sub(family.timestamp_valid_bits);
        let mask = (unused_bits.filter(|&unused| unused < u64::BITS))
            .and_then(|unused| u64::MAX.checked_shr(unused))
            .ok_or(Error::NoTimestamps)?;
        let info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(QUERIES);
        // SAFETY: ash 0.38 has no safe form of vkCreateQueryPool. The device is live through its `Arc`, and the
        // create info has no chain.
        let handle = unsafe { device.ash().create_query_pool(&info, None) }
            .map_err(Error::CreateQueryPool)?;
        let queries = QueryPool {
            device: device.clone(),
            handle,
        };
        Ok(Self {
            start: Commands::new(device)?,
            stop: Commands::new(device)?,
            clock: Clock {
                queries,
                period: f64::from(physical.properties().limits.timestamp_period),
                mask,
            },
        })
    }

    /// Submits commands that reset both queries and write the start timestamp once the work submitted before them is
    /// complete.
    ///
    /// # Errors
    /// Fails when the driver cannot record or submit.
    pub fn start(self) -> Result<RunningTimer, Error> {
        let recording = self.start.record()?;
        self.clock.reset(&recording);
        self.clock.write(&recording, START);
        Ok(RunningTimer {
            start: recording.finish()?.submit()?,
            stop: self.stop,
            clock: self.clock,
        })
    }
}

impl RunningTimer {
    /// Submits a command that writes the stop timestamp once the work submitted before it is complete.
    ///
    /// # Errors
    /// Fails when the driver cannot record or submit.
    pub fn stop(self) -> Result<StoppedTimer, Error> {
        let recording = self.stop.record()?;
        self.clock.write(&recording, STOP);
        Ok(StoppedTimer {
            stop: recording.finish()?.submit()?,
            start: self.start,
            clock: self.clock,
        })
    }
}

impl StoppedTimer {
    /// # Errors
    /// Fails when the device is lost.
    pub fn reading(&self) -> Result<Reading, Error> {
        if self.stop.status()? == Status::Pending {
            return Ok(Reading::Pending);
        }
        let mut ticks = [0_u64; QUERIES as usize];
        let device = self.clock.queries.device.ash();
        let flags = vk::QueryResultFlags::TYPE_64;
        // SAFETY: ash 0.38 has no safe form of vkGetQueryPoolResults. Both queries belong to the live pool, `ticks`
        // has room for one 64-bit result for each, and the submissions that write them are complete.
        unsafe {
            device.get_query_pool_results(self.clock.queries.handle, START, &mut ticks, flags)
        }
        .map_err(Error::QueryResults)?;
        Ok(Reading::Elapsed(
            self.clock
                .elapsed(ticks[START as usize], ticks[STOP as usize]),
        ))
    }

    /// Waits until both submissions of the timer are complete and returns the timer, ready to start again.
    ///
    /// # Errors
    /// Fails when the device is lost.
    pub fn reset(self) -> Result<GpuTimer, Error> {
        Ok(GpuTimer {
            start: self.start.reclaim()?,
            stop: self.stop.reclaim()?,
            clock: self.clock,
        })
    }
}

impl Clock {
    fn reset(&self, recording: &Recording) {
        let device = recording.device().ash();
        // SAFETY: ash 0.38 has no safe form of vkCmdResetQueryPool. The command buffer is recording. The pool belongs
        // to the device of the buffer, contains both queries and outlives the submissions of the buffer.
        unsafe {
            device.cmd_reset_query_pool(recording.buffer(), self.queries.handle, START, QUERIES);
        }
    }

    fn write(&self, recording: &Recording, query: u32) {
        let device = recording.device().ash();
        let stage = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
        // SAFETY: ash 0.38 has no safe form of vkCmdWriteTimestamp. The command buffer is recording. The queue family
        // supports timestamps, as `new` checked. The start commands reset the query, and they run before the stop
        // commands.
        unsafe {
            device.cmd_write_timestamp(recording.buffer(), stage, self.queries.handle, query);
        }
    }

    fn elapsed(&self, start: u64, stop: u64) -> Duration {
        let ticks = stop.wrapping_sub(start) & self.mask;
        Duration::from_nanos(ticks).mul_f64(self.period)
    }
}

impl Drop for QueryPool {
    fn drop(&mut self) {
        // SAFETY: ash 0.38 has no safe form of vkDestroyQueryPool. The pool belongs to the live device. The clock that
        // contains the pool drops after the submissions that use the pool, and a submission waits for its work to
        // complete when it drops.
        unsafe { self.device.ash().destroy_query_pool(self.handle, None) };
    }
}
