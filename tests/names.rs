use avrotize::common::names::{
    avro_name, avro_name_with_altname, avro_namespace, camel, get_typing_args_from_string, pascal,
    snake,
};
use regex::Regex;

/// Input, `avro_name`, its altname, `avro_namespace`, `pascal`, `camel`, `snake`,
/// as returned by 0.1.1.
type NameCase = (
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

#[rustfmt::skip]
const NAMES: &[NameCase] = &[
    ("", "_", Some(""), "", "", "", ""),
    ("a", "a", None, "a", "A", "a", "a"),
    ("_", "_", None, "_", "_", "", "_"),
    ("1abc", "_1abc", Some("1abc"), "_1abc", "1abc", "1abc", "1abc"),
    ("9", "_9", Some("9"), "_9", "9", "9", "9"),
    ("a-b", "a_b", Some("a-b"), "a_b", "AB", "aB", "a_b"),
    ("a.b", "a_b", Some("a.b"), "a.b", "A.B", "a.b", "a.b"),
    (".a", "_a", Some(".a"), ".a", ".A", ".a", ".a"),
    ("a b\tc", "a_b_c", Some("a b\tc"), "a_b_c", "ABC", "aBC", "a_b_c"),
    ("$ref", "_ref", Some("$ref"), "_ref", "Ref", "ref", "ref"),
    ("日本", "__", Some("日本"), "__", "", "", ""),
    ("é", "_", Some("é"), "_", "", "", ""),
    ("a\u{301}", "a_", Some("a\u{301}"), "a_", "A", "a", "a"),
    ("😀x", "_x", Some("😀x"), "_x", "X", "x", "x"),
    ("ABC", "ABC", None, "ABC", "ABC", "aBC", "a_b_c"),
    ("abcDef", "abcDef", None, "abcDef", "AbcDef", "abcDef", "abc_def"),
    ("AbcDef.Ghi", "AbcDef_Ghi", Some("AbcDef.Ghi"), "AbcDef.Ghi", "AbcDef.Ghi", "abcDef.ghi", "abc_def.ghi"),
    ("a_b_c", "a_b_c", None, "a_b_c", "ABC", "aBC", "a_b_c"),
    ("a::b::C", "a__b__C", Some("a::b::C"), "a__b__C", "a::B::C", "a::b::c", "a::b::c"),
    ("x.", "x_", Some("x."), "x.", "X.", "x.", "x."),
    ("en-gb", "en_gb", Some("en-gb"), "en_gb", "EnGb", "enGb", "en_gb"),
    ("zh-hant", "zh_hant", Some("zh-hant"), "zh_hant", "ZhHant", "zhHant", "zh_hant"),
    ("_private", "_private", None, "_private", "_Private", "Private", "_private"),
    ("Foo_bar", "Foo_bar", None, "Foo_bar", "FooBar", "fooBar", "foo_bar"),
    ("fooBarBaz", "fooBarBaz", None, "fooBarBaz", "FooBarBaz", "fooBarBaz", "foo_bar_baz"),
    ("Foo::barBaz::Qux_quux", "Foo__barBaz__Qux_quux", Some("Foo::barBaz::Qux_quux"), "Foo__barBaz__Qux_quux", "Foo::BarBaz::QuxQuux", "Foo::barBaz::quxQuux", "Foo::bar_baz::qux_quux"),
    ("HTTPServer", "HTTPServer", None, "HTTPServer", "HTTPServer", "hTTPServer", "h_t_t_p_server"),
    ("a1B2", "a1B2", None, "a1B2", "A1B2", "a1B2", "a1_b2"),
];

#[test]
fn names_match_0_1_1() {
    for &(input, name, alt, namespace, pascal_case, camel_case, snake_case) in NAMES {
        assert_eq!(avro_name(input), name, "avro_name({input:?})");
        assert_eq!(
            avro_name_with_altname(input),
            (name.to_string(), alt.map(str::to_string)),
            "avro_name_with_altname({input:?})"
        );
        assert_eq!(
            avro_namespace(input),
            namespace,
            "avro_namespace({input:?})"
        );
        assert_eq!(pascal(input), pascal_case, "pascal({input:?})");
        assert_eq!(camel(input), camel_case, "camel({input:?})");
        assert_eq!(snake(input), snake_case, "snake({input:?})");
    }
}

#[test]
fn common_reexports_names() {
    for &(input, ..) in NAMES {
        assert_eq!(avrotize::common::avro_name(input), avro_name(input));
        assert_eq!(
            avrotize::common::avro_name_with_altname(input),
            avro_name_with_altname(input)
        );
        assert_eq!(
            avrotize::common::avro_namespace(input),
            avro_namespace(input)
        );
    }
}

/// `avro_name` as in 0.1.1, compiling its regex on each call.
fn avro_name_0_1_1(name: &str) -> String {
    let mut val = Regex::new(r"[^a-zA-Z0-9_]")
        .unwrap()
        .replace_all(name, "_")
        .to_string();
    if val
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        val = format!("_{}", val);
    }
    if val.is_empty() || !val.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        val = format!("_{}", val);
    }
    val
}

/// `avro_namespace` as in 0.1.1, compiling its regex on each call.
fn avro_namespace_0_1_1(name: &str) -> String {
    let mut val = Regex::new(r"[^a-zA-Z0-9_\.]")
        .unwrap()
        .replace_all(name, "_")
        .to_string();
    if val
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        val = format!("_{}", val);
    }
    val
}

#[test]
fn names_match_0_1_1_on_generated_strings() {
    const ALPHABET: &[char] = &[
        'a', 'z', 'A', 'Z', '0', '9', '_', '.', '-', ' ', '\t', '$', ':', 'é', 'ß', '日', '😀',
        '\u{301}', '\0', '\u{FFFF}', 'İ',
    ];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..2_000 {
        let len = (next() % 12) as usize;
        let s: String = (0..len)
            .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize])
            .collect();
        assert_eq!(avro_name(&s), avro_name_0_1_1(&s), "avro_name({s:?})");
        assert_eq!(
            avro_namespace(&s),
            avro_namespace_0_1_1(&s),
            "avro_namespace({s:?})"
        );
    }
}

#[test]
fn typing_args() {
    assert_eq!(get_typing_args_from_string("List[int]"), ["int"]);
    assert_eq!(
        get_typing_args_from_string("Dict[str, List[int]]"),
        ["str", "List[int]"]
    );
    assert_eq!(
        get_typing_args_from_string("a.b.C[x, y[z, w], v]"),
        ["x", "y[z, w]", "v"]
    );
    assert_eq!(get_typing_args_from_string("Ünï[x]"), ["x"]);
    assert!(get_typing_args_from_string("int").is_empty());
}
