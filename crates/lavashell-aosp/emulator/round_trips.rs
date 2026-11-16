// SPDX-License-Identifier: GPL-3.0-only
use crate::error::NativeMethodError;
use crate::stream_round_trip::StreamRoundTrip;
use ::jni::sys::jlong;
use std::cell::RefCell;

#[derive(Default)]
pub struct RoundTrips(Vec<Option<StreamRoundTrip>>);

impl RoundTrips {
    pub fn insert(&mut self, round_trip: StreamRoundTrip) -> Result<jlong, NativeMethodError> {
        let handle = jlong::try_from(self.0.len()).map_err(NativeMethodError::TooManyRoundTrips)?;
        self.0.push(Some(round_trip));
        Ok(handle)
    }

    pub fn get_mut(&mut self, handle: jlong) -> Result<&mut StreamRoundTrip, NativeMethodError> {
        self.slot(handle)?
            .as_mut()
            .ok_or(NativeMethodError::UnknownHandle(handle))
    }

    pub fn take(&mut self, handle: jlong) -> Result<StreamRoundTrip, NativeMethodError> {
        self.slot(handle)?
            .take()
            .ok_or(NativeMethodError::UnknownHandle(handle))
    }

    fn slot(&mut self, handle: jlong) -> Result<&mut Option<StreamRoundTrip>, NativeMethodError> {
        usize::try_from(handle)
            .ok()
            .and_then(|index| self.0.get_mut(index))
            .ok_or(NativeMethodError::UnknownHandle(handle))
    }
}

thread_local! {
    static ROUND_TRIPS: RefCell<RoundTrips> = RefCell::default();
}

pub fn with_round_trips<T>(
    work: impl FnOnce(&mut RoundTrips) -> Result<T, NativeMethodError>,
) -> Result<T, NativeMethodError> {
    ROUND_TRIPS.with(|round_trips| {
        let mut round_trips = round_trips
            .try_borrow_mut()
            .map_err(NativeMethodError::RoundTripsInUse)?;
        work(&mut round_trips)
    })
}
