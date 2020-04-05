//! Integ tests are separated into their own "crate" so that they can depend on
//! extra features, without obscuring what the crate actually depends on during development.
//! (Cargo has the habit of adding the test features in with regular build tests.)
