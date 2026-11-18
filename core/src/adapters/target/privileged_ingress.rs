include!("privileged_ingress/install.rs");
include!("privileged_ingress/validation.rs");
include!("privileged_ingress/platform.rs");
include!("privileged_ingress/selinux.rs");

#[cfg(test)]
#[path = "privileged_ingress/tests.rs"]
mod validation_tests;
