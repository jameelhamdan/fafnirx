use super::*;
use tonic::Code;

fn told(e: impl Into<CallError>) -> Code {
    Status::from(e.into()).code()
}

#[test]
fn what_a_call_fails_with_decides_what_the_client_is_told() {
    assert_eq!(told(Status::not_found("x")), Code::NotFound, "a status is told as it is");
    assert_eq!(told(ApiError::conflict("taken")), Code::AlreadyExists);
    assert_eq!(told(ApiError::bad("no")), Code::InvalidArgument);
    assert_eq!(
        told(ApiError::not_found("no")),
        Code::Internal,
        "the rest of a web refusal is not the client's to know"
    );
    assert_eq!(told(anyhow::anyhow!("disk")), Code::Internal);
    assert_eq!(told(std::io::Error::other("disk")), Code::Internal);
}

#[test]
fn a_refusal_inside_an_anyhow_error_is_found() {
    let wrapped: anyhow::Error = ApiError::conflict("a shared folder cannot hold that path").into();
    let e = Status::from(CallError::from(wrapped));
    assert_eq!((e.code(), e.message()), (Code::AlreadyExists, "a shared folder cannot hold that path"));
    assert_eq!(told(anyhow::Error::from(Status::data_loss("x"))), Code::DataLoss);
}

#[test]
fn an_internal_error_does_not_leak_its_text() {
    let e = Status::from(CallError::Internal(anyhow::anyhow!("password=hunter2")));
    assert_eq!(e.message(), "internal error");
}
