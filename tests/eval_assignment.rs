#![deny(rust_2018_idioms)]

use conch_runtime::eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};

mod support;
pub use self::support::*;

#[derive(Debug, Clone)]
struct MockWordCfg {
    cfg: WordEvalConfig,
    fields: Fields<String>,
}

#[async_trait::async_trait]
impl<E> WordEval<E> for MockWordCfg
where
    E: ?Sized + Send + Sync,
{
    type EvalResult = String;
    type Error = MockErr;

    async fn eval_with_config(
        &self,
        _: &mut E,
        cfg: WordEvalConfig,
    ) -> WordEvalResult<Self::EvalResult, Self::Error> {
        assert_eq!(cfg, self.cfg);
        let ret = self.fields.clone();
        Ok(Box::pin(async move { ret }))
    }
}

#[tokio::test]
async fn test_eval_as_assignment_expands_all_tilde_and_does_not_split_words() {
    use conch_runtime::env::{VarEnv, VariableEnvironment};

    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: false,
    };

    let mut env = VarEnv::new();
    env.set_var("IFS".to_owned(), "!".to_owned());

    {
        let word = MockWordCfg {
            cfg,
            fields: Fields::Zero,
        };
        let mut env = env.clone();
        assert_eq!(eval_as_assignment(word, &mut env).await, Ok("".to_owned()));
    }

    {
        let msg = "foo".to_owned();
        let word = MockWordCfg {
            cfg,
            fields: Fields::Single(msg.clone()),
        };
        let mut env = env.clone();
        assert_eq!(eval_as_assignment(word, &mut env).await, Ok(msg));
    }

    {
        let word = MockWordCfg {
            cfg,
            fields: Fields::At(vec!["foo".to_owned(), "bar".to_owned()]),
        };

        let mut env = env.clone();
        assert_eq!(
            eval_as_assignment(word, &mut env).await,
            Ok("foo bar".to_owned())
        );
    }

    {
        let word = MockWordCfg {
            cfg,
            fields: Fields::Split(vec!["foo".to_owned(), "bar".to_owned()]),
        };

        let mut env = env.clone();
        assert_eq!(
            eval_as_assignment(word, &mut env).await,
            Ok("foo bar".to_owned())
        );
    }

    {
        let word = MockWordCfg {
            cfg,
            fields: Fields::Star(vec!["foo".to_owned(), "bar".to_owned()]),
        };

        let mut env = env.clone();
        assert_eq!(
            eval_as_assignment(word, &mut env).await,
            Ok("foo!bar".to_owned())
        );
    }
}
