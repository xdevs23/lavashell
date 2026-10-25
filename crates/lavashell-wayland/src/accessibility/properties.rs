// SPDX-License-Identifier: GPL-3.0-only
use super::{ids::Ids, mapping};
use accesskit::{Invalid, Live, Node, Toggled};
use lavashell_core::accessibility::{AccessibleNode, AccessibleValue, Checked, LiveRegion, Role};

pub(super) fn apply(source: &AccessibleNode, node: &mut Node, ids: &mut Ids) {
    if let Some(name) = &source.name {
        node.set_label(name.clone());
    }
    if let Role::Custom(description) = &source.role {
        node.set_role_description(description.clone());
    }
    if let Some(description) = &source.description {
        node.set_description(description.clone());
    }
    if let Some(level) = source.heading_level {
        // Lavashell numbers heading levels from 1, as HTML does. AccessKit numbers them from 0.
        node.set_level(usize::from(level.saturating_sub(1)));
    }
    match &source.value {
        Some(AccessibleValue::Text(value)) => node.set_value(value.clone()),
        Some(AccessibleValue::Number(value)) if value.is_finite() => node.set_numeric_value(*value),
        _ => {}
    }
    // The bounds are in logical coordinates of the surface, because a Wayland client cannot learn where its window is
    // on the screen.
    node.set_bounds(mapping::rect(
        source.visible_bounds.unwrap_or(source.bounds),
    ));
    let state = &source.state.semantics;
    if state.disabled {
        node.set_disabled();
    }
    if state.form.read_only {
        node.set_read_only();
    }
    if state.form.required {
        node.set_required();
    }
    if state.form.invalid {
        node.set_invalid(Invalid::True);
    }
    if state.busy {
        node.set_busy();
    }
    if state.modal {
        node.set_modal();
    }
    if let Some(checked) = state.checked {
        node.set_toggled(match checked {
            Checked::False => Toggled::False,
            Checked::True => Toggled::True,
            Checked::Mixed => Toggled::Mixed,
        });
    }
    if let Some(selected) = source.state.selected {
        node.set_selected(selected);
    }
    if let Some(expanded) = state.expanded {
        node.set_expanded(expanded);
    }
    node.set_live(match source.live {
        LiveRegion::Off => Live::Off,
        LiveRegion::Polite => Live::Polite,
        LiveRegion::Assertive => Live::Assertive,
    });
    if let Some(range) = source
        .range
        .filter(lavashell_core::accessibility::RangeInfo::valid)
    {
        node.set_min_numeric_value(range.min);
        node.set_max_numeric_value(range.max);
        if let Some(step) = range.step {
            node.set_numeric_value_step(step);
        }
    }
    relations(source, node, ids);
    collections(source, node);
    mapping::actions(source, node);
}

fn relations(source: &AccessibleNode, node: &mut Node, ids: &mut Ids) {
    let relations = &source.relations;
    node.set_labelled_by(
        relations
            .labelled_by
            .iter()
            .map(|id| ids.node(*id))
            .collect::<Vec<_>>(),
    );
    node.set_described_by(
        relations
            .described_by
            .iter()
            .map(|id| ids.node(*id))
            .collect::<Vec<_>>(),
    );
    node.set_controls(
        relations
            .controls
            .iter()
            .map(|id| ids.node(*id))
            .collect::<Vec<_>>(),
    );
    // An AccessKit node has one error message relation. The first error message uses that relation, and every further
    // error message is added as a description of the node.
    if let Some(id) = relations.error_message.first() {
        node.set_error_message(ids.node(*id));
        for id in relations.error_message.iter().skip(1) {
            node.push_described_by(ids.node(*id));
        }
    }
    if let Some(member) = relations.member_of {
        node.set_member_of(ids.node(member));
    }
}

fn collections(source: &AccessibleNode, node: &mut Node) {
    if let Some(collection) = source.collection {
        if let Some(rows) = collection.rows {
            node.set_row_count(rows);
        }
        if let Some(columns) = collection.columns {
            node.set_column_count(columns);
        }
        if collection.multiple_selection {
            node.set_multiselectable();
        }
        if matches!(source.role, Role::List)
            && let Some(count) = collection.item_count.or_else(|| {
                collection
                    .rows?
                    .checked_mul(collection.columns.unwrap_or(1))
            })
        {
            node.set_size_of_set(count);
        }
    }
    if let Some(item) = source.item {
        node.set_row_index(item.row);
        node.set_column_index(item.column);
        node.set_row_span(item.row_span.max(1));
        node.set_column_span(item.column_span.max(1));
        node.set_position_in_set(item.row);
    }
}
