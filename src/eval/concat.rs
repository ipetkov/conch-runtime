use env::StringWrapper;
use future::{Async, EnvFuture, Poll};
use eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};
use std::fmt;
use std::iter::Fuse;
use std::mem;

/// Creates a future adapter which concatenates multiple words together.
///
/// All words will be concatenated together in the same field, however,
/// intermediate `At`, `Star`, and `Split` fields will be handled as follows:
/// the first newly generated field will be concatenated to the last existing
/// field, and the remainder of the newly generated fields will form their own
/// distinct fields.
pub fn concat<W, I, E: ?Sized>(words: I, env: &E, cfg: WordEvalConfig) -> Concat<W, I::IntoIter, E>
    where W: WordEval<E>,
          I: IntoIterator<Item = W>,
{
    let mut iter = words.into_iter().fuse();
    let future = iter.next().map(|w| w.eval_with_config(env, cfg));

    Concat {
        split_fields_further: cfg.split_fields_further,
        fields: Vec::new(),
        future: future,
        rest: iter,
    }
}

/// A future adapter which concatenates multiple words together.
///
/// All words will be concatenated together in the same field, however,
/// intermediate `At`, `Star`, and `Split` fields will be handled as follows:
/// the first newly generated field will be concatenated to the last existing
/// field, and the remainder of the newly generated fields will form their own
/// distinct fields.
#[must_use = "futures do nothing unless polled"]
pub struct Concat<W, I, E: ?Sized>
    where W: WordEval<E>,
          I: Iterator<Item = W>,
{
    split_fields_further: bool,
    fields: Vec<W::EvalResult>,
    future: Option<W::EvalFuture>,
    rest: Fuse<I>,
}

impl<W, I, E: ?Sized> fmt::Debug for Concat<W, I, E>
    where W: WordEval<E> + fmt::Debug,
          W::EvalResult: fmt::Debug,
          W::EvalFuture: fmt::Debug,
          I: Iterator<Item = W> + fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Concat")
            .field("split_fields_further", &self.split_fields_further)
            .field("fields", &self.fields)
            .field("future", &self.future)
            .field("rest", &self.rest)
            .finish()
    }
}

impl<W, I, E: ?Sized> EnvFuture<E> for Concat<W, I, E>
    where W: WordEval<E>,
          I: Iterator<Item = W>,
{
    type Item = Fields<W::EvalResult>;
    type Error = W::Error;

    // FIXME: implement tilde substitution here somehow?
    // FIXME: might also be useful to publicly expose the `concat` function once implemented
    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            if self.future.is_none() {
                if let Some(w) = self.rest.next() {
                    let cfg = WordEvalConfig {
                        tilde_expansion: TildeExpansion::None,
                        split_fields_further: self.split_fields_further,
                    };

                    self.future = Some(w.eval_with_config(env, cfg));
                }
            }

            let next = match self.future {
                None => {
                    let fields = mem::replace(&mut self.fields, Vec::new());
                    return Ok(Async::Ready(fields.into()));
                },

                Some(ref mut f) => try_ready!(f.poll(env)),
            };

            // Ensure we don't poll twice
            self.future = None;

            let mut iter = next.into_iter().fuse();
            match (self.fields.pop(), iter.next()) {
                (Some(last), Some(next)) => {
                    let mut new = last.into_owned();
                    new.push_str(next.as_str());
                    self.fields.push(new.into());
                },
                (Some(last), None) => self.fields.push(last),
                (None, Some(next)) => self.fields.push(next),
                (None, None) => continue,
            }

            self.fields.extend(iter);
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.future.as_mut().map(|f| f.cancel(env));
    }
}
