use crate::env::StringWrapper;
use crate::eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};
use futures_core::future::BoxFuture;
use std::iter::{Fuse, Peekable};

/// Creates a future adapter which concatenates multiple words together.
///
/// All words will be concatenated together in the same field, however,
/// intermediate `At`, `Star`, and `Split` fields will be handled as follows:
/// the first newly generated field will be concatenated to the last existing
/// field, and the remainder of the newly generated fields will form their own
/// distinct fields.
pub async fn concat<I, E>(
    words: I,
    env: &mut E,
    cfg: WordEvalConfig,
) -> Result<
    BoxFuture<'static, Fields<<I::Item as WordEval<E>>::EvalResult>>,
    <I::Item as WordEval<E>>::Error,
>
where
    I: IntoIterator,
    I::Item: WordEval<E>,
    <I::Item as WordEval<E>>::EvalResult: 'static + Send,
    E: ?Sized,
{
    do_concat(words.into_iter().fuse().peekable(), env, cfg).await
}

async fn do_concat<W, I, E>(
    mut words: Peekable<Fuse<I>>,
    env: &mut E,
    cfg: WordEvalConfig,
) -> Result<BoxFuture<'static, Fields<W::EvalResult>>, W::Error>
where
    W: WordEval<E>,
    W::EvalResult: 'static + Send,
    I: Iterator<Item = W>,
    E: ?Sized,
{
    // FIXME: implement tilde substitution here somehow?
    let mut fields = match words.next() {
        None => vec![],
        Some(first_word) => {
            let future = first_word.eval_with_config(env, cfg).await?;
            if words.peek().is_none() {
                // No more words return our result as is
                return Ok(Box::pin(future));
            } else {
                match future.await {
                    Fields::Zero => vec![],
                    Fields::Single(s) => vec![s],
                    Fields::At(v) | Fields::Star(v) | Fields::Split(v) => v,
                }
            }
        }
    };

    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::None,
        split_fields_further: cfg.split_fields_further,
    };

    let mut last = None;
    while let Some(word) = words.next() {
        let future = word.eval_with_config(env, cfg).await?;

        // If this is the last word, we can continue without the environment
        if words.peek().is_none() {
            last = Some(future);
            break;
        }

        append(&mut fields, future.await);
    }

    Ok(Box::pin(async move {
        if let Some(future) = last {
            append(&mut fields, future.await);
        }

        Fields::from(fields)
    }))
}

fn append<T: StringWrapper>(previous: &mut Vec<T>, next: Fields<T>) {
    let mut iter = next.into_iter().fuse();

    if let Some(next) = iter.next() {
        match previous.pop() {
            None => previous.push(next),
            Some(last) => {
                let mut new = last.into_owned();
                new.push_str(next.as_str());
                previous.push(new.into());
            }
        }
    }

    previous.extend(iter);
}
