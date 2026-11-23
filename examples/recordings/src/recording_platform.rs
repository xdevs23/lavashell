// SPDX-License-Identifier: GPL-3.0-only
use lavashell::vulkan_renderer::HeadlessBackend;
use lavashell_test_platform::TestPlatform;

pub type RecordingPlatform = TestPlatform<HeadlessBackend>;

#[cfg(test)]
mod tests;
