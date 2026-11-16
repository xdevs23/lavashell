// SPDX-License-Identifier: GPL-3.0-only
use crate::animation::{Animatable, Trajectory, VectorSpace};

pub(super) fn blend<T: Animatable>(from: &T, to: &T, progress: f64) -> T {
    let end = to.spanning(from);
    let start = Trajectory::at_rest(from.vector());
    let Trajectory {
        position,
        end: target,
        ..
    } = end.retarget(from, start);
    end.at(position.add(&target.sub(&position).scale(progress)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::samples::BLUE;
    use crate::scene::{FontWeight, TextStyle};
    use crate::style::InteractionStyle;
    use crate::test_error::{Required, TestResult};
    use crate::ui::{Node, NodeId, NodeKind};
    use crate::units::{Dp, SpExt};
    use crate::view::transition::VisualAppearance;

    fn title(weight: FontWeight) -> TextStyle<Dp> {
        TextStyle::new("sans-serif", 16.sp(), BLUE)
            .weight(weight)
            .resolve_unscaled()
    }

    fn shown_as(style: TextStyle<Dp>) -> VisualAppearance {
        let kind = NodeKind::Text("Display".into(), style);
        VisualAppearance::of(&Node::new(NodeId::ROOT, kind, InteractionStyle::default()))
    }

    #[test]
    fn a_quarter_of_the_progress_moves_the_text_weight_a_quarter_of_the_way() -> TestResult {
        let header = shown_as(title(FontWeight::try_new(800.0)?));
        let row = shown_as(title(FontWeight::NORMAL));
        let text = blend(&header, &row, 0.25).text.required("text blend")?;
        let style = text.applied_to(&title(FontWeight::NORMAL));
        assert_eq!(style.weight, FontWeight::try_new(700.0)?);
        Ok(())
    }
}
