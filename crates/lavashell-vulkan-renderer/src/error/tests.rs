// SPDX-License-Identifier: GPL-3.0-only
use super::{ContextState, Error};
use ash::vk;
use lavashell_error_chain::next_step::NamesNextStep;
use lavashell_renderer::DrawFailure;

#[test]
fn an_error_of_the_vulkan_renderer_takes_the_next_step_of_the_error_it_wraps() {
    let painter = lavashell_renderer::Error::SubpixelTileSize;
    let painter_step = painter.next_step();
    assert_eq!(Error::CreatePainter(painter).next_step(), painter_step);
    let vulkan = lavashell_vulkan::Error::Submit(vk::Result::ERROR_OUT_OF_DEVICE_MEMORY);
    let vulkan_step = vulkan.next_step();
    assert_eq!(Error::SubmitCompletion(vulkan).next_step(), vulkan_step);
    let failure = DrawFailure::NumberRange;
    assert_eq!(
        Error::OutputColorFilter(failure).next_step(),
        failure.next_step()
    );
    let context = ContextState::DeviceLost;
    assert_eq!(
        Error::WrapSwapchainImage(context).next_step(),
        context.next_step()
    );
}
