// SPDX-License-Identifier: GPL-3.0-only
use super::Error;
use ash::vk;
use lavashell_error_chain::next_step::NamesNextStep;

#[test]
fn a_failed_vulkan_call_takes_its_next_step_from_the_result_whatever_the_call() {
    for result in [
        vk::Result::ERROR_OUT_OF_HOST_MEMORY,
        vk::Result::ERROR_OUT_OF_DEVICE_MEMORY,
        vk::Result::ERROR_DEVICE_LOST,
        vk::Result::ERROR_SURFACE_LOST_KHR,
    ] {
        assert_eq!(
            Error::Submit(result).next_step(),
            Error::Present(result).next_step(),
            "{result:?}"
        );
    }
    assert_ne!(
        Error::Submit(vk::Result::ERROR_OUT_OF_HOST_MEMORY).next_step(),
        Error::Submit(vk::Result::ERROR_OUT_OF_DEVICE_MEMORY).next_step()
    );
}
