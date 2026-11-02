//! CEL's lists extension, as cel-go declares it: `flatten`, `slice`, `reverse`, `distinct`, `sort`, `lists.range`, and the hidden function `sortBy` expands to. cel-core has none, so the interpreter runs no body.

use cel_core::{CelType, FunctionDecl, OverloadDecl};

pub(super) fn declarations() -> Vec<FunctionDecl> {
    let t = || CelType::type_param("T");
    let of_t = || CelType::list(t());
    let method = |id: &str, mut params: Vec<CelType>, result: CelType| {
        params.insert(0, of_t());
        OverloadDecl::method(id, params, result).with_type_params(vec!["T".to_string()])
    };
    let flatten = OverloadDecl::method("list_flatten", vec![CelType::list(of_t())], of_t())
        .with_type_params(vec!["T".to_string()]);
    let flatten_depth = OverloadDecl::method(
        "list_flatten_depth",
        vec![CelType::list(CelType::Dyn), CelType::Int],
        CelType::list(CelType::Dyn),
    );
    let sort_by_keys = OverloadDecl::function(
        "list_sort_by_keys",
        vec![of_t(), CelType::list(CelType::type_param("K"))],
        of_t(),
    )
    .with_type_params(vec!["T".to_string(), "K".to_string()]);
    vec![
        FunctionDecl::new(crate::macros::SORT_BY_KEYS).with_overload(sort_by_keys),
        FunctionDecl::new("flatten").with_overloads([flatten, flatten_depth]),
        FunctionDecl::new("slice").with_overload(method(
            "list_slice",
            vec![CelType::Int, CelType::Int],
            of_t(),
        )),
        FunctionDecl::new("reverse").with_overload(method("list_reverse", vec![], of_t())),
        FunctionDecl::new("distinct").with_overload(method("list_distinct", vec![], of_t())),
        FunctionDecl::new("sort").with_overload(method("list_sort", vec![], of_t())),
        FunctionDecl::new("lists.range").with_overload(OverloadDecl::function(
            "lists_range",
            vec![CelType::Int],
            CelType::list(CelType::Int),
        )),
    ]
}
