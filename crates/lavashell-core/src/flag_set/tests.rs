// SPDX-License-Identifier: GPL-3.0-only

flag_set!(pub enum Light in Lights { Red, Green, Blue });

#[test]
fn the_set_of_all_flags_lists_each_in_declaration_order_and_the_default_set_is_empty() {
    let all: Vec<Light> = Lights::all().iter().collect();
    assert_eq!(all, [Light::Red, Light::Green, Light::Blue]);
    assert!(Lights::default().is_empty());
}

#[test]
fn adding_or_removing_a_flag_leaves_the_other_flags_as_they_were() {
    let green = Lights::new([Light::Green]);
    let green_and_blue = green.with(Light::Blue);
    assert_eq!(green_and_blue, Lights::new([Light::Green, Light::Blue]));
    assert_eq!(
        green_and_blue.without(Light::Green),
        Lights::new([Light::Blue])
    );
    assert_eq!(green.with(Light::Green), green);
    assert_eq!(green.without(Light::Red), green);
}
