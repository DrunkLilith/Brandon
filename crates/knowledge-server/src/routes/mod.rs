macro_rules! api_router {
    (
        $visibility:vis fn $router:ident;
        $(
            $path:literal => [
                $first_method:ident => $first_handler:path => ($first_id:literal, $first_access:ident, $first_success:literal)
                $(, $method:ident => $handler:path => ($id:literal, $access:ident, $success:literal))* $(,)?
            ]
        ),+ $(,)?
    ) => {
        pub(crate) const OPERATIONS: &[crate::openapi::OperationSpec] = &[
            $(
                crate::openapi::OperationSpec::new(
                    $path,
                    stringify!($first_method),
                    $first_id,
                    crate::openapi::Access::$first_access,
                    $first_success,
                ),
                $(crate::openapi::OperationSpec::new(
                    $path,
                    stringify!($method),
                    $id,
                    crate::openapi::Access::$access,
                    $success,
                ),)*
            )+
        ];

        $visibility fn $router() -> axum::Router<crate::state::AppState> {
            axum::Router::new()$(.route(
                $path,
                axum::routing::$first_method($first_handler)$(.$method($handler))*,
            ))+
        }
    };
}

pub(crate) mod attachments;
pub(crate) mod audit;
pub(crate) mod auth;
pub(crate) mod exports;
pub(crate) mod groups;
pub(crate) mod health;
pub(crate) mod operational;
pub(crate) mod pages;
pub(crate) mod search;
pub(crate) mod spaces;
pub(crate) mod users;

pub(crate) fn operation_specs() -> impl Iterator<Item = &'static crate::openapi::OperationSpec> {
    [
        health::OPERATIONS,
        operational::OPERATIONS,
        auth::OPERATIONS,
        users::OPERATIONS,
        groups::OPERATIONS,
        spaces::OPERATIONS,
        pages::OPERATIONS,
        search::OPERATIONS,
        attachments::OPERATIONS,
        audit::OPERATIONS,
        exports::OPERATIONS,
    ]
    .into_iter()
    .flatten()
}
