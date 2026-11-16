// SPDX-License-Identifier: GPL-3.0-only
use crate::{Commands, Device, Error, Recording, Status, Submitted};
use ash::vk;
use std::sync::Arc;
use std::time::Duration;

pub struct GpuTimer {
    start: Commands,
    stop: Commands,
    queries: TimestampQueries,
}

pub struct RunningTimer {
    start: Submitted,
    stop: Commands,
    queries: TimestampQueries,
}

pub struct StoppedTimer {
    start: Submitted,
    stop: Submitted,
    queries: TimestampQueries,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Pending,
    Elapsed(Duration),
}

struct TimestampQueries {
    pool: QueryPool,
    period: TickPeriod,
    mask: u64,
}

#[derive(Clone, Copy)]
struct TickPeriod(f64);

#[derive(Clone, Copy)]
struct Ticks(u64);

#[derive(Clone, Copy)]
enum Timestamp {
    Start,
    Stop,
}

struct QueryPool {
    device: Arc<Device>,
    handle: vk::QueryPool,
}

impl Timestamp {
    const COUNT: u32 = 2;

    fn index(self) -> u32 {
        match self {
            Self::Start => 0,
            Self::Stop => 1,
        }
    }
}

impl TryFrom<f32> for TickPeriod {
    type Error = Error;

    fn try_from(nanoseconds: f32) -> Result<Self, Error> {
        let nanoseconds = f64::from(nanoseconds);
        if nanoseconds.is_normal() && nanoseconds.is_sign_positive() {
            Ok(Self(nanoseconds))
        } else {
            Err(Error::TimestampPeriod(nanoseconds))
        }
    }
}

impl Ticks {
    fn duration(self, period: TickPeriod) -> Result<Duration, Error> {
        let at_one_nanosecond_per_tick = Duration::from_nanos(self.0);
        Duration::try_from_secs_f64(at_one_nanosecond_per_tick.as_secs_f64() * period.0)
            .map_err(Error::TimestampDuration)
    }
}

impl GpuTimer {
    pub fn new(device: &Arc<Device>) -> Result<Self, Error> {
        let physical = device.physical_device();
        let families = physical.queue_families();
        let family = families
            .get(device.queue_family() as usize)
            .ok_or(Error::QueueFamily(device.queue_family()))?;
        let period = TickPeriod::try_from(physical.properties().limits.timestamp_period)?;
        let unused_bits = u64::BITS.checked_sub(family.timestamp_valid_bits);
        let mask = (unused_bits.filter(|&unused| unused < u64::BITS))
            .and_then(|unused| u64::MAX.checked_shr(unused))
            .ok_or(Error::NoTimestamps)?;
        let info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(Timestamp::COUNT);
        // SAFETY: ash 0.38 has no safe form of vkCreateQueryPool. The device is live through its `Arc`, and the
        // create info has no chain.
        let handle = unsafe { device.ash().create_query_pool(&info, None) }
            .map_err(Error::CreateQueryPool)?;
        let pool = QueryPool {
            device: device.clone(),
            handle,
        };
        Ok(Self {
            start: Commands::new(device)?,
            stop: Commands::new(device)?,
            queries: TimestampQueries { pool, period, mask },
        })
    }

    pub fn start(self) -> Result<RunningTimer, Error> {
        let recording = self.start.record()?;
        self.queries.reset(&recording);
        self.queries.write(&recording, Timestamp::Start);
        Ok(RunningTimer {
            start: recording.finish()?.submit()?,
            stop: self.stop,
            queries: self.queries,
        })
    }
}

impl RunningTimer {
    pub fn stop(self) -> Result<StoppedTimer, Error> {
        let recording = self.stop.record()?;
        self.queries.write(&recording, Timestamp::Stop);
        Ok(StoppedTimer {
            stop: recording.finish()?.submit()?,
            start: self.start,
            queries: self.queries,
        })
    }
}

impl StoppedTimer {
    pub fn reading(&self) -> Result<Reading, Error> {
        if self.stop.status()? == Status::Pending {
            return Ok(Reading::Pending);
        }
        let mut results = [0_u64; Timestamp::COUNT as usize];
        let device = self.queries.pool.device.ash();
        let flags = vk::QueryResultFlags::TYPE_64;
        // SAFETY: ash 0.38 has no safe form of vkGetQueryPoolResults. Both queries belong to the live pool, `results`
        // has room for one 64-bit result for each, and the submissions that write them are complete.
        unsafe {
            device.get_query_pool_results(
                self.queries.pool.handle,
                Timestamp::Start.index(),
                &mut results,
                flags,
            )
        }
        .map_err(Error::QueryResults)?;
        let [start, stop] = results.map(Ticks);
        let elapsed = self.queries.elapsed(start, stop)?;
        Ok(Reading::Elapsed(elapsed))
    }

    pub fn reset(self) -> Result<GpuTimer, Error> {
        Ok(GpuTimer {
            start: self.start.reclaim()?,
            stop: self.stop.reclaim()?,
            queries: self.queries,
        })
    }
}

impl TimestampQueries {
    fn reset(&self, recording: &Recording) {
        let device = recording.device().ash();
        // SAFETY: ash 0.38 has no safe form of vkCmdResetQueryPool. The command buffer is recording. The pool belongs
        // to the device of the buffer, contains both queries and outlives the submissions of the buffer.
        unsafe {
            device.cmd_reset_query_pool(
                recording.buffer(),
                self.pool.handle,
                Timestamp::Start.index(),
                Timestamp::COUNT,
            );
        }
    }

    fn write(&self, recording: &Recording, timestamp: Timestamp) {
        let device = recording.device().ash();
        let stage = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
        // SAFETY: ash 0.38 has no safe form of vkCmdWriteTimestamp. The command buffer is recording. The queue family
        // supports timestamps, as `new` checked. The start commands reset the query, and they run before the stop
        // commands.
        unsafe {
            device.cmd_write_timestamp(
                recording.buffer(),
                stage,
                self.pool.handle,
                timestamp.index(),
            );
        }
    }

    fn elapsed(&self, start: Ticks, stop: Ticks) -> Result<Duration, Error> {
        Ticks(stop.0.wrapping_sub(start.0) & self.mask).duration(self.period)
    }
}

impl Drop for QueryPool {
    fn drop(&mut self) {
        // SAFETY: ash 0.38 has no safe form of vkDestroyQueryPool. The pool belongs to the live device. The
        // `TimestampQueries` that contain the pool drop after the submissions that use the pool, and a submission
        // waits for its work to complete when it drops.
        unsafe { self.device.ash().destroy_query_pool(self.handle, None) };
    }
}
