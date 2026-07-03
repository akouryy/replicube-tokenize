use replicube_tokenize::{Warning, tokenize};

fn size(src: &str) -> Option<usize> { tokenize(src).0.iter().map(|t| t.cost()).sum() }

fn warns(src: &str) -> Vec<Warning> { tokenize(src).1 }

#[test]
fn string_cost_is_two_to_the_floor_half_length() {
    assert_eq!(size(r#"return z=="abc" and 1"#), Some(7));
    assert_eq!(size(r#"return z=="abcde" and 1"#), Some(9));
}

#[test]
fn table_and_indexing() {
    assert_eq!(size("list={abs(x),abs(y),abs(z)} return list[2]"), Some(18));
}

#[test]
fn comma_open_bracket_merges_only_when_adjacent() {
    assert_eq!(size("({2,[abs(x)]=3})[y]"), Some(11));
    assert_eq!(size("({2, [abs(x)]=3})[y]"), Some(12));
}

#[test]
fn open_bracket_ends_a_value() {
    // `[-5]` lexes as `[`, `-`, `5` (subtraction), so it costs one more than `[5]`.
    assert_eq!(size("({[-5]=7})[a]"), Some(9));
    assert_eq!(size("({[5]=7})[a]"), Some(8));
}

#[test]
fn full_program_size() {
    let p = "return x==-2 and y==-1-t and z==2 and 7 or x//2==1 and y==-2 and z|2==-1 and 5-z/2 or \
             y<=-2 and (x/-4>=-2-y and 13 or 9) or x==2 and y//-2==0 and z==-2 and 3 or \
             x//-3==-1 and y==1 and z//3==-1 and 3*(-1)^(x+y+z)+4 or x+y+z==12 and 9";
    assert_eq!(size(p), Some(111));
}

#[test]
fn semicolon_cost_is_undetermined() {
    assert_eq!(size("a=1 b=2"), Some(6));
    assert_eq!(size("a=1;b=2"), None);
}

#[test]
fn warns_about_spaced_comma_before_bracket() {
    assert!(matches!(warns("({2, [3]=4})[y]").as_slice(), [Warning::WhitespaceBetweenCommaAndBracket { .. }]));
    assert!(warns("({2,[3]=4})[y]").is_empty());
}

#[test]
fn warns_about_semicolon() {
    assert!(matches!(warns("a=1;b=2").as_slice(), [Warning::Semicolon { .. }]));
}
