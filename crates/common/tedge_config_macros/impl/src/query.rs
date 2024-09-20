use crate::error::extract_type_from_result;
use crate::input::ConfigurableField;
use crate::input::FieldOrGroup;
use heck::ToUpperCamelCase;
use itertools::Itertools;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use std::collections::VecDeque;
use syn::parse_quote;
use syn::parse_quote_spanned;
use syn::spanned::Spanned;

pub fn generate_writable_keys(items: &[FieldOrGroup]) -> TokenStream {
    let paths = configuration_paths_from(items);
    let (readonly_destr, write_error): (Vec<_>, Vec<_>) = paths
        .iter()
        .filter_map(|field| {
            let configuration = variant_name(field);
            Some((
                configuration.match_shape,
                field
                    .back()?
                    .field()?
                    .read_only()?
                    .readonly
                    .write_error
                    .as_str(),
            ))
        })
        .multiunzip();
    let readable_args = configuration_strings(paths.iter());
    let readonly_args = configuration_strings(paths.iter().filter(|path| !is_read_write(path)));
    let writable_args = configuration_strings(paths.iter().filter(|path| is_read_write(path)));
    let readable_keys = keys_enum(parse_quote!(ReadableKey), &readable_args, "read from");
    let readonly_keys = keys_enum(
        parse_quote!(ReadOnlyKey),
        &readonly_args,
        "read from, but not written to,",
    );
    let writable_keys = keys_enum(parse_quote!(WritableKey), &writable_args, "written to");
    let fromstr_readable = generate_fromstr_readable(parse_quote!(ReadableKey), &readable_args);
    let fromstr_readonly = generate_fromstr_readable(parse_quote!(ReadOnlyKey), &readonly_args);
    let fromstr_writable = generate_fromstr_writable(parse_quote!(WritableKey), &writable_args);
    let read_string = generate_string_readers(&paths);
    let write_string = generate_string_writers(
        &paths
            .iter()
            .filter(|path| is_read_write(path))
            .cloned()
            .collect::<Vec<_>>(),
    );
    let (static_alias, deprecated_keys) = deprecated_keys(paths.iter());
    let iter_updated = deprecated_keys.iter().map(|k| &k.iter_field);

    let fallback_branch: Option<syn::Arm> = readonly_args
        .0
        .is_empty()
        .then(|| parse_quote!(_ => unreachable!("ReadOnlyKey is uninhabited")));

    quote! {
        #readable_keys
        #readonly_keys
        #writable_keys
        #fromstr_readable
        #fromstr_readonly
        #fromstr_writable
        #read_string
        #write_string

        #[derive(::thiserror::Error, Debug)]
        /// An error encountered when writing to a configuration value from a
        /// string
        pub enum WriteError {
            #[error("Failed to parse input")]
            ParseValue(#[from] Box<dyn ::std::error::Error + Send + Sync>),
            #[error(transparent)]
            Multi(#[from] ::tedge_config_macros::MultiError),
        }

        impl ReadOnlyKey {
            fn write_error(&self) -> &'static str {
                match self {
                    #(Self::#readonly_destr => #write_error,)*
                    #fallback_branch
                }
            }
        }

        #[derive(Debug, ::thiserror::Error)]
        /// An error encountered when parsing a configuration key from a string
        pub enum ParseKeyError {
            #[error("{}", .0.write_error())]
            ReadOnly(ReadOnlyKey),
            #[error("Unknown key: '{0}'")]
            Unrecognised(String),
        }

        fn replace_aliases(key: String) -> String {
            use ::once_cell::sync::Lazy;
            use ::std::borrow::Cow;
            use ::std::collections::HashMap;
            use ::doku::*;

            static ALIASES: Lazy<HashMap<Cow<'static, str>, Cow<'static, str>>> = Lazy::new(|| {
                let ty = TEdgeConfigReader::ty();
                let TypeKind::Struct { fields, transparent: false } = ty.kind else { panic!("Expected struct but got {:?}", ty.kind) };
                let Fields::Named { fields } = fields else { panic!("Expected named fields but got {:?}", fields)};
                let mut aliases = struct_field_aliases(None, &fields);
                #(
                    if let Some(alias) = aliases.insert(Cow::Borrowed(#static_alias), Cow::Borrowed(ReadableKey::#iter_updated.as_str())) {
                        panic!("Duplicate configuration alias for '{}'. It maps to both '{}' and '{}'. Perhaps you provided an incorrect `deprecated_key` for one of these configurations?", #static_alias, alias, ReadableKey::#iter_updated.as_str());
                    }
                )*
                aliases
            });

            ALIASES
                .get(&Cow::Borrowed(key.as_str()))
                .map(|c| c.clone().into_owned())
                .unwrap_or(key)
        }

        fn warn_about_deprecated_key(deprecated_key: String, updated_key: &'static str) {
            use ::once_cell::sync::Lazy;
            use ::std::sync::Mutex;
            use ::std::collections::HashSet;

            static WARNINGS: Lazy<Mutex<HashSet<String>>> = Lazy::new(<_>::default);

            let warning = format!("The key '{}' is deprecated. Use '{}' instead.", deprecated_key, updated_key);
            if WARNINGS.lock().unwrap().insert(deprecated_key) {
                ::tracing::warn!("{}", warning);
            }
        }
    }
}

fn configuration_strings<'a>(
    variants: impl Iterator<Item = &'a VecDeque<&'a FieldOrGroup>>,
) -> (Vec<String>, Vec<ConfigurationKey>) {
    variants
        .map(|segments| {
            let configuration_key = variant_name(segments);
            (
                segments
                    .iter()
                    .map(|variant| variant.name())
                    .collect::<Vec<_>>()
                    .join("."),
                configuration_key,
            )
        })
        .unzip()
}

fn deprecated_keys<'a>(
    variants: impl Iterator<Item = &'a VecDeque<&'a FieldOrGroup>>,
) -> (Vec<&'a str>, Vec<ConfigurationKey>) {
    variants
        .flat_map(|segments| {
            segments
                .back()
                .unwrap()
                .field()
                .unwrap()
                .deprecated_keys()
                .map(|key| {
                    let configuration_key = variant_name(segments);
                    (key, configuration_key)
                })
        })
        .multiunzip()
}

fn generate_fromstr(
    type_name: syn::Ident,
    (configuration_string, configuration_key): &(Vec<String>, Vec<ConfigurationKey>),
    error_case: syn::Arm,
) -> TokenStream {
    let simplified_configuration_string = configuration_string
        .iter()
        .map(|s| s.replace('.', "_"))
        .zip(configuration_key.iter().map(|k| &k.enum_variant))
        .map(|(s, v)| quote_spanned!(v.span()=> #s));
    let iter_variant = configuration_key.iter().map(|k| &k.iter_field);

    // TODO oh shit make this actually work!
    quote! {
        impl ::std::str::FromStr for #type_name {
            type Err = ParseKeyError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                // If we get an unreachable pattern, it means we have the same key twice
                #[deny(unreachable_patterns)]
                match replace_aliases(value.to_owned()).replace(".", "_").as_str() {
                    #(
                        #simplified_configuration_string => {
                            if (value != #configuration_string) {
                                warn_about_deprecated_key(value.to_owned(), #configuration_string);
                            }
                            Ok(Self::#iter_variant)
                        },
                    )*
                    #error_case
                }
            }
        }
    }
}

fn generate_fromstr_readable(
    type_name: syn::Ident,
    fields: &(Vec<String>, Vec<ConfigurationKey>),
) -> TokenStream {
    generate_fromstr(
        type_name,
        fields,
        parse_quote! { _ => Err(ParseKeyError::Unrecognised(value.to_owned())) },
    )
}

// TODO test the error messages actually appear
fn generate_fromstr_writable(
    type_name: syn::Ident,
    fields: &(Vec<String>, Vec<ConfigurationKey>),
) -> TokenStream {
    generate_fromstr(
        type_name,
        fields,
        parse_quote! {
            _ => if let Ok(key) = <ReadOnlyKey as ::std::str::FromStr>::from_str(value) {
                Err(ParseKeyError::ReadOnly(key))
            } else {
                Err(ParseKeyError::Unrecognised(value.to_owned()))
            },
        },
    )
}

fn keys_enum(
    type_name: syn::Ident,
    (configuration_string, configuration_key): &(Vec<String>, Vec<ConfigurationKey>),
    doc_fragment: &'static str,
) -> TokenStream {
    let as_str_example = configuration_key
        .iter()
        .map(|k| &k.iter_field)
        .zip(configuration_string.iter())
        .map(|(ident, value)| {
            format!(
                "assert_eq!({type_name}::{ident}.as_str(), \"{value}\");\n",
                ident = quote!(#ident)
            )
        })
        .take(10)
        .collect::<Vec<_>>();
    let as_str_example = (!as_str_example.is_empty()).then(|| {
        quote! {
            /// ```compile_fail
            /// // This doctest is compile_fail because we have no way to import the
            /// // current type, but the example is still valuable
            #(
                #[doc = #as_str_example]
            )*
            /// ```
        }
    });
    let type_name_str = type_name.to_string();
    let enum_variant = configuration_key.iter().map(|k| &k.enum_variant);
    let match_shape = configuration_key.iter().map(|k| &k.match_shape);
    let iter_field = configuration_key.iter().map(|k| &k.iter_field);

    quote! {
        #[derive(Clone, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        #[allow(unused)]
        #[doc = concat!("A key that can be *", #doc_fragment, "* the configuration\n\n")]
        #[doc = concat!("This can be converted to `&'static str` using [`", #type_name_str, "::as_str`], and")]
        #[doc = "parsed using [`FromStr`](::std::str::FromStr). The `FromStr` implementation also"]
        #[doc = "automatically emits warnings about deprecated keys. It also implements [Display](std::fmt::Display),"]
        #[doc = "so you can also use it in format strings."]
        pub enum #type_name {
            #(
                #[doc = concat!("`", #configuration_string, "`")]
                #enum_variant,
            )*
        }

        impl #type_name {
            /// Converts this key to the canonical key used by `tedge config` and `tedge.toml`
            #as_str_example
            pub fn as_str(&self) -> &'static str {
                match self {
                    #(
                        Self::#match_shape => #configuration_string,
                    )*
                    // TODO make this conditional
                    _ => unimplemented!("Cope with empty enum")
                }
            }

            /// Iterates through all the variants of this enum
            pub fn iter() -> impl Iterator<Item = Self> {
                [
                    #(
                        Self::#iter_field,
                    )*
                ].into_iter()
            }
        }

        impl ::std::fmt::Display for #type_name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                self.as_str().fmt(f)
            }
        }
    }
}

#[derive(Debug, Default)]
struct SequentialIdGenerator {
    count: u32,
}

#[derive(Debug, Default)]
struct UnderscoreIdGenerator;

pub trait IdGenerator: Default {
    fn next_id(&mut self, span: Span) -> syn::Ident;
}

impl IdGenerator for SequentialIdGenerator {
    fn next_id(&mut self, span: Span) -> syn::Ident {
        let i = self.count;
        self.count += 1;
        syn::Ident::new(&format!("key{i}"), span)
    }
}

impl Iterator for SequentialIdGenerator {
    type Item = syn::Ident;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.next_id(Span::call_site()))
    }
}

impl IdGenerator for UnderscoreIdGenerator {
    fn next_id(&mut self, span: Span) -> syn::Ident {
        syn::Ident::new("_", span)
    }
}

impl Iterator for UnderscoreIdGenerator {
    type Item = syn::Ident;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.next_id(Span::call_site()))
    }
}

fn generate_field_accessor<'a>(
    fields: &'a VecDeque<&FieldOrGroup>,
    method: &'a str,
) -> impl Iterator<Item = TokenStream> + 'a {
    let mut id_gen = SequentialIdGenerator::default();
    let method = syn::Ident::new(method, Span::call_site());
    fields.iter().map(move |field| {
        let ident = field.ident();
        match field {
            FieldOrGroup::Field(_) => quote!(#ident),
            FieldOrGroup::Group(_) => quote!(#ident),
            FieldOrGroup::Multi(_) => {
                let field = id_gen.next_id(ident.span());
                quote_spanned!(ident.span()=> #ident.#method(#field.as_deref())?)
            }
        }
    })
}

fn generate_string_readers(paths: &[VecDeque<&FieldOrGroup>]) -> TokenStream {
    let variant_names = paths.iter().map(variant_name);
    let arms = paths
        .iter()
        .zip(variant_names)
        .map(|(path, configuration_key)| -> syn::Arm {
            let field = path
                .back()
                .expect("Path must have a back as it is nonempty")
                .field()
                .expect("Back of path is guaranteed to be a field");
            let segments = generate_field_accessor(path, "get");
            let to_string = quote_spanned!(field.ty().span()=> .to_string());
            let match_variant = configuration_key.match_read_write;
            if field.read_only().is_some() {
                if extract_type_from_result(field.ty()).is_some() {
                    // TODO test whether the wrong type fails unit tests
                    parse_quote! {
                        ReadableKey::#match_variant => Ok(self.#(#segments).*.try_read(self)?#to_string),
                    }
                } else {
                    parse_quote! {
                        ReadableKey::#match_variant => Ok(self.#(#segments).*.read(self)#to_string),
                    }
                }
            } else if field.has_guaranteed_default() {
                parse_quote! {
                    ReadableKey::#match_variant => Ok(self.#(#segments).*#to_string),
                }
            } else {
                parse_quote! {
                    ReadableKey::#match_variant => Ok(self.#(#segments).*.or_config_not_set()?#to_string),
                }
            }
        });
    let fallback_branch: Option<syn::Arm> = paths
        .is_empty()
        .then(|| parse_quote!(_ => unreachable!("ReadableKey is uninhabited")));
    quote! {
        impl TEdgeConfigReader {
            pub fn read_string(&self, key: &ReadableKey) -> Result<String, ReadError> {
                match key {
                    #(#arms)*
                    #fallback_branch
                }
            }
        }
    }
}

fn generate_string_writers(paths: &[VecDeque<&FieldOrGroup>]) -> TokenStream {
    let variant_names = paths.iter().map(variant_name);
    let (update_arms, unset_arms, append_arms, remove_arms): (
        Vec<syn::Arm>,
        Vec<syn::Arm>,
        Vec<syn::Arm>,
        Vec<syn::Arm>,
    ) = paths
        .iter()
        .zip(variant_names)
        .map(|(path, configuration_key)| {
            let read_segments = generate_field_accessor(path, "get");
            let write_segments = generate_field_accessor(path, "get_mut").collect::<Vec<_>>();
            let field = path
                .iter()
                .filter_map(|thing| thing.field())
                .next()
                .unwrap();
            let match_variant = configuration_key.match_read_write;

            let ty = field.ty();
            let parse_as = field.from().unwrap_or(field.ty());
            let parse = quote_spanned! {parse_as.span()=> parse::<#parse_as>() };
            let convert_to_field_ty = quote_spanned! {ty.span()=> map(<#ty>::from)};

            let current_value = if field.read_only().is_some() {
                if extract_type_from_result(field.ty()).is_some() {
                    quote_spanned! {ty.span()=> reader.#(#read_segments).*.try_read(reader).ok()}
                } else {
                    quote_spanned! {ty.span()=> Some(reader.#(#read_segments).*.read(reader))}
                }
            } else if field.has_guaranteed_default() {
                quote_spanned! {ty.span()=> Some(reader.#(#read_segments).*.to_owned())}
            } else {
                quote_spanned! {ty.span()=> reader.#(#read_segments).*.or_none().cloned()}
            };

            (
                parse_quote_spanned! {ty.span()=>
                    WritableKey::#match_variant => self.#(#write_segments).* = Some(value
                        .#parse
                        .#convert_to_field_ty
                        .map_err(|e| WriteError::ParseValue(Box::new(e)))?),
                },
                parse_quote_spanned! {ty.span()=>
                    WritableKey::#match_variant => self.#(#write_segments).* = None,
                },
                parse_quote_spanned! {ty.span()=>
                    WritableKey::#match_variant => self.#(#write_segments).* = <#ty as AppendRemoveItem>::append(
                        #current_value,
                        value
                        .#parse
                        .#convert_to_field_ty
                        .map_err(|e| WriteError::ParseValue(Box::new(e)))?),
                },
                parse_quote_spanned! {ty.span()=>
                    WritableKey::#match_variant => self.#(#write_segments).* = <#ty as AppendRemoveItem>::remove(
                        #current_value,
                        value
                        .#parse
                        .#convert_to_field_ty
                        .map_err(|e| WriteError::ParseValue(Box::new(e)))?),
                },
            )
        })
        .multiunzip();
    let fallback_branch: Option<syn::Arm> = update_arms
        .is_empty()
        .then(|| parse_quote!(_ => unreachable!("WritableKey is uninhabited")));

    quote! {
        impl TEdgeConfigDto {
            pub fn try_update_str(&mut self, key: &WritableKey, value: &str) -> Result<(), WriteError> {
                match key {
                    #(#update_arms)*
                    #fallback_branch
                };
                Ok(())
            }

            pub fn try_unset_key(&mut self, key: &WritableKey) -> Result<(), WriteError> {
                match key {
                    #(#unset_arms)*
                    #fallback_branch
                };
                Ok(())
            }

            pub fn try_append_str(&mut self, reader: &TEdgeConfigReader, key: &WritableKey, value: &str) -> Result<(), WriteError> {
                match key {
                    #(#append_arms)*
                    #fallback_branch
                };
                Ok(())
            }

            pub fn try_remove_str(&mut self, reader: &TEdgeConfigReader, key: &WritableKey, value: &str) -> Result<(), WriteError> {
                match key {
                    #(#remove_arms)*
                    #fallback_branch
                };
                Ok(())
            }
        }
    }
}

struct ConfigurationKey {
    /// e.g. `C8yUrl(Option<String>)`
    enum_variant: syn::Variant,
    // TODO kill this when it's not used
    iter_field: syn::Expr,
    /// e.g. `C8yUrl(key0)`
    match_read_write: syn::Pat,
    /// e.g. `C8yUrl(_)`
    match_shape: syn::Pat,
}

fn ident_for(segments: &VecDeque<&FieldOrGroup>) -> syn::Ident {
    syn::Ident::new(
        &segments
            .iter()
            .map(|segment| segment.name().to_upper_camel_case())
            .collect::<String>(),
        segments.iter().last().unwrap().ident().span(),
    )
}

fn variant_name(segments: &VecDeque<&FieldOrGroup>) -> ConfigurationKey {
    let ident = ident_for(segments);
    let count_multi = segments
        .iter()
        .filter(|fog| matches!(fog, FieldOrGroup::Multi(_)))
        .count();
    if count_multi > 0 {
        let opt_strs =
            std::iter::repeat::<syn::Type>(parse_quote!(Option<String>)).take(count_multi);
        let enum_variant = parse_quote_spanned!(ident.span()=> #ident(#(#opt_strs),*));
        let nones = std::iter::repeat::<syn::Path>(parse_quote!(None)).take(count_multi);
        let iter_field = parse_quote_spanned!(ident.span()=> #ident(#(#nones),*));
        let var_idents = SequentialIdGenerator::default().take(count_multi);
        let match_read_write = parse_quote_spanned!(ident.span()=> #ident(#(#var_idents),*));
        let underscores = UnderscoreIdGenerator.take(count_multi);
        let match_shape = parse_quote_spanned!(ident.span()=> #ident(#(#underscores),*));
        ConfigurationKey {
            enum_variant,
            iter_field,
            match_shape,
            match_read_write,
        }
    } else {
        ConfigurationKey {
            enum_variant: parse_quote!(#ident),
            iter_field: parse_quote!(#ident),
            match_read_write: parse_quote!(#ident),
            match_shape: parse_quote!(#ident),
        }
    }
}

/// Generates a list of the toml paths for each of the keys in the provided
/// configuration
fn configuration_paths_from(items: &[FieldOrGroup]) -> Vec<VecDeque<&FieldOrGroup>> {
    let mut res = vec![];
    for item in items.iter().filter(|item| !item.reader().skip) {
        match item {
            FieldOrGroup::Field(_) => res.push(VecDeque::from([item])),
            FieldOrGroup::Group(group) | FieldOrGroup::Multi(group) => {
                for mut fields in configuration_paths_from(&group.contents) {
                    fields.push_front(item);
                    res.push(fields);
                }
            }
        }
    }
    res
}

/// Checks if the field for the given path is read write
fn is_read_write(path: &VecDeque<&FieldOrGroup>) -> bool {
    matches!(
        path.back(), // the field
        Some(FieldOrGroup::Field(ConfigurableField::ReadWrite(_))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_parses() {
        syn::parse2::<syn::File>(generate_writable_keys(&[])).unwrap();
    }

    #[test]
    fn output_parses_for_multi() {
        let input: crate::input::Configuration = parse_quote! {
            #[tedge_config(multi)]
            c8y: {
                url: String
            }
        };
        syn::parse2::<syn::File>(generate_writable_keys(&input.groups)).unwrap();
    }
}
