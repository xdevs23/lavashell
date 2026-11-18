// SPDX-License-Identifier: GPL-3.0-only
use lavashell_renderer::Painter;

pub type StreamTask<S> = Box<dyn FnOnce(&mut S, &Painter) + Send>;
