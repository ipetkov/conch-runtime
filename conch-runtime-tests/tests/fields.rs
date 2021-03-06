#![deny(rust_2018_idioms)]

use conch_runtime::env::{UnsetVariableEnvironment, VarEnv, VariableEnvironment};
use conch_runtime::eval::Fields::*;

#[tokio::test]
async fn test_fields_is_null() {
    let empty_strs = vec!["".to_owned(), "".to_owned(), "".to_owned()];

    let mostly_non_empty_strs = vec!["foo".to_owned(), "".to_owned(), "bar".to_owned()];

    assert_eq!(Zero::<String>.is_null(), true);
    assert_eq!(Single("".to_owned()).is_null(), true);
    assert_eq!(At(empty_strs.clone()).is_null(), true);
    assert_eq!(Star(empty_strs.clone()).is_null(), true);
    assert_eq!(Split(empty_strs).is_null(), true);

    assert_eq!(Single("foo".to_owned()).is_null(), false);
    assert_eq!(At(mostly_non_empty_strs.clone()).is_null(), false);
    assert_eq!(Star(mostly_non_empty_strs.clone()).is_null(), false);
    assert_eq!(Split(mostly_non_empty_strs).is_null(), false);
}

#[tokio::test]
async fn test_fields_join() {
    let strs = vec!["foo".to_owned(), "".to_owned(), "bar".to_owned()];

    assert_eq!(Zero::<String>.join(), "");
    assert_eq!(Single("foo".to_owned()).join(), "foo");
    assert_eq!(At(strs.clone()).join(), "foo bar");
    assert_eq!(Star(strs.clone()).join(), "foo bar");
    assert_eq!(Split(strs).join(), "foo bar");
}

#[tokio::test]
async fn test_fields_join_with_ifs() {
    let ifs = "IFS".to_owned();
    let strs = vec![
        "foo".to_owned(),
        "".to_owned(), // Empty strings should not be eliminated
        "bar".to_owned(),
    ];

    let mut env = VarEnv::new();

    env.set_var(ifs.clone(), "!".to_owned());
    assert_eq!(Zero::<String>.join_with_ifs(&env), "");
    assert_eq!(Single("foo".to_owned()).join_with_ifs(&env), "foo");
    assert_eq!(At(strs.clone()).join_with_ifs(&env), "foo!!bar");
    assert_eq!(Star(strs.clone()).join_with_ifs(&env), "foo!!bar");
    assert_eq!(Split(strs.clone()).join_with_ifs(&env), "foo!!bar");

    // Blank IFS
    env.set_var(ifs.clone(), "".to_owned());
    assert_eq!(Zero::<String>.join_with_ifs(&env), "");
    assert_eq!(Single("foo".to_owned()).join_with_ifs(&env), "foo");
    assert_eq!(At(strs.clone()).join_with_ifs(&env), "foobar");
    assert_eq!(Star(strs.clone()).join_with_ifs(&env), "foobar");
    assert_eq!(Split(strs.clone()).join_with_ifs(&env), "foobar");

    env.unset_var(&ifs);
    assert_eq!(Zero::<String>.join_with_ifs(&env), "");
    assert_eq!(Single("foo".to_owned()).join_with_ifs(&env), "foo");
    assert_eq!(At(strs.clone()).join_with_ifs(&env), "foo  bar");
    assert_eq!(Star(strs.clone()).join_with_ifs(&env), "foo  bar");
    assert_eq!(Split(strs).join_with_ifs(&env), "foo  bar");
}

#[tokio::test]
async fn test_fields_from_vec() {
    let s = "foo".to_owned();
    let strs = vec![s.clone(), "".to_owned(), "bar".to_owned()];

    assert_eq!(Zero::<String>, Vec::<String>::new().into());
    assert_eq!(Single(s.clone()), vec!(s).into());
    assert_eq!(Split(strs.clone()), strs.into());
}

#[tokio::test]
async fn test_fields_from_t() {
    let string = "foo".to_owned();
    assert_eq!(Single(string.clone()), string.into());
    // Empty string is NOT an empty field
    let string = "".to_owned();
    assert_eq!(Single(string.clone()), string.into());
}

#[tokio::test]
async fn test_fields_into_iter() {
    let s = "foo".to_owned();
    let strs = vec![s.clone(), "".to_owned(), "bar".to_owned()];

    let empty: Vec<String> = vec![];
    assert_eq!(empty, Zero::<String>.into_iter().collect::<Vec<_>>());
    assert_eq!(vec!(s.clone()), Single(s).into_iter().collect::<Vec<_>>());
    assert_eq!(
        strs.clone(),
        At(strs.clone()).into_iter().collect::<Vec<_>>()
    );
    assert_eq!(
        strs.clone(),
        Star(strs.clone()).into_iter().collect::<Vec<_>>()
    );
    assert_eq!(strs.clone(), Split(strs).into_iter().collect::<Vec<_>>());
}

#[tokio::test]
async fn test_eval_parameter_substitution_splitting_default_ifs() {
    let mut env = VarEnv::<String, String>::new();
    env.unset_var(&String::from("IFS"));

    // Splitting SHOULD keep empty fields between IFS chars which are NOT whitespace
    assert_eq!(
        Single(" \t\nfoo \t\nbar \t\n".to_owned()).split(&env),
        Split(vec!("foo".to_owned(), "bar".to_owned(),))
    );

    assert_eq!(Single("".to_owned()).split(&env), Zero);
}

#[tokio::test]
async fn test_splitting_with_custom_ifs() {
    let mut env = VarEnv::new();
    env.set_var("IFS".to_owned(), "0 ".to_owned());

    // Splitting SHOULD keep empty fields between IFS chars which are NOT whitespace
    assert_eq!(
        Single("   foo000bar   ".to_owned()).split(&env),
        Split(vec!(
            "foo".to_owned(),
            "".to_owned(),
            "".to_owned(),
            "bar".to_owned(),
        ))
    );

    assert_eq!(
        Single("  00 0 00  0 ".to_owned()).split(&env),
        Split(vec!(
            "".to_owned(),
            "".to_owned(),
            "".to_owned(),
            "".to_owned(),
            "".to_owned(),
            "".to_owned(),
        ))
    );

    assert_eq!(Single("".to_owned()).split(&env), Zero);
}

#[tokio::test]
async fn test_no_splitting_if_ifs_blank() {
    let mut env = VarEnv::new();
    env.set_var("IFS".to_owned(), "".to_owned());

    let fields = Single(" \t\nfoo \t\nbar \t\n".to_owned());
    assert_eq!(fields.clone().split(&env), fields);
}
