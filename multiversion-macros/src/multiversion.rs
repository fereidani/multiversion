use crate::dispatcher::{DispatchMethod, Dispatcher};
use crate::target::Target;
use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{
    parenthesized, parse::Parser, parse_quote, punctuated::Punctuated, spanned::Spanned, token,
    Attribute, Error, ItemFn, LitStr, Meta, ReturnType, Type,
};

// Default set of targets that are selected when `targets = "simd"` is specified.
static DEFAULT_TARGETS: &[&str] = &[
    // "x86_64+avx512f+avx512bw+avx512cd+avx512dq+avx512vl",
    "x86_64+avx2+fma",
    "x86_64+sse4.2",
    // "x86+avx512f+avx512bw+avx512cd+avx512dq+avx512vl",
    "x86+avx2+fma",
    "x86+sse4.2",
    "x86+sse2",
    "aarch64+neon",
    // "arm+neon",
    // "mips+msa",
    // "mips64+msa",
    // "powerpc+vsx",
    // "powerpc+altivec",
    // "powerpc64+vsx",
    // "powerpc64+altivec",
];

fn parse_targets(
    meta: syn::meta::ParseNestedMeta,
    targets: &mut Option<Vec<Target>>,
) -> Result<(), syn::Error> {
    if targets.is_some() {
        return Err(meta.error("can't specify `targets` multiple times"));
    }

    if meta.input.peek(token::Paren) {
        let content;
        parenthesized!(content in meta.input);
        *targets = Some(
            Punctuated::<Target, token::Comma>::parse_terminated(&content)?
                .into_iter()
                .collect(),
        );
        Ok(())
    } else {
        let value = meta.value()?;
        let s: LitStr = value.parse()?;

        if s.value().as_str() == "simd" {
            *targets = Some(
                DEFAULT_TARGETS
                    .iter()
                    .map(|x| Target::parse(&LitStr::new(x, meta.path.span())).unwrap())
                    .collect(),
            );
            Ok(())
        } else {
            Err(meta.error("expected a list of features or \"simd\""))
        }
    }
}

fn parse_attrs(
    meta: syn::meta::ParseNestedMeta,
    inner_attrs: &mut Option<Vec<Attribute>>,
) -> Result<(), syn::Error> {
    if inner_attrs.is_some() {
        return Err(meta.error("can't specify `attrs` multiple times"));
    }
    *inner_attrs = Some(Vec::new());
    let content;
    parenthesized!(content in meta.input);
    *inner_attrs = Some(
        Punctuated::<Meta, token::Comma>::parse_terminated(&content)?
            .into_iter()
            .map(|meta| parse_quote! { #[#meta] })
            .collect(),
    );
    Ok(())
}

fn parse_dispatcher(
    meta: syn::meta::ParseNestedMeta,
    dispatcher: &mut Option<DispatchMethod>,
) -> Result<(), syn::Error> {
    if dispatcher.is_some() {
        return Err(meta.error("can't specify `dispatcher` multiple times"));
    }
    let value = meta.value()?;
    let s: LitStr = value.parse()?;
    *dispatcher = Some(match s.value().as_str() {
        "default" => DispatchMethod::Default,
        "static" => DispatchMethod::Static,
        "direct" => DispatchMethod::Direct,
        "indirect" => DispatchMethod::Indirect,
        _ => return Err(meta.error("expected `default`, `static`, `direct`, or `indirect`")),
    });
    Ok(())
}

pub(crate) fn make_multiversioned_fn(
    attr: TokenStream,
    func: ItemFn,
) -> Result<TokenStream, syn::Error> {
    if let ReturnType::Type(_, ty) = &func.sig.output {
        if let Type::ImplTrait(_) = **ty {
            return Err(Error::new(
                ty.span(),
                "cannot multiversion function with `impl Trait` return type",
            ));
        }
    }

    let mut targets: Option<Vec<Target>> = None;
    let mut inner_attrs: Option<Vec<Attribute>> = None;
    let mut dispatcher: Option<DispatchMethod> = None;

    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("targets") {
            parse_targets(meta, &mut targets)
        } else if meta.path.is_ident("attrs") {
            parse_attrs(meta, &mut inner_attrs)
        } else if meta.path.is_ident("dispatcher") {
            parse_dispatcher(meta, &mut dispatcher)
        } else {
            Err(meta.error("unrecognized option"))
        }
    });

    let span = attr.span();
    parser.parse2(attr)?;

    let targets = if let Some(targets) = targets {
        for target in targets.iter() {
            if !target.has_features_specified() {
                // TODO add span to Target
                return Err(Error::new(span, "target must have features specified"));
            }
        }
        targets
    } else {
        return Err(Error::new(span, "expected `targets`"));
    };

    let inner_attrs = inner_attrs.unwrap_or_default();
    let dispatcher = dispatcher.unwrap_or(DispatchMethod::Default);

    Ok(Dispatcher {
        targets,
        func,
        inner_attrs,
        dispatcher,
    }
    .to_token_stream())
}
