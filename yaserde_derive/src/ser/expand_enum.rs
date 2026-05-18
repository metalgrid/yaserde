use crate::common::{Field, YaSerdeAttribute, YaSerdeField};
use crate::ser::{implement_serializer::implement_serializer, label::build_label_name};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Fields;
use syn::Ident;
use syn::{DataEnum, Generics};

pub fn serialize(
  data_enum: &DataEnum,
  name: &Ident,
  root: &str,
  root_attributes: &YaSerdeAttribute,
  generics: &Generics,
) -> TokenStream {
  let inner_enum_inspector = inner_enum_inspector(data_enum, name, root_attributes);

  let get_id = |field: &YaSerdeField| {
    field
      .label()
      .unwrap_or(field.get_type().get_simple_type_visitor())
  };

  let variant_matches: TokenStream = data_enum
    .variants
    .iter()
    .map(|variant| -> TokenStream {
      let add_tag = if let Some(tag) = &root_attributes.tag {
        let attrs = crate::common::YaSerdeAttribute::from(&variant.attrs);
        let label = variant.ident.clone();
        let element_name = attrs.xml_element_name(&variant.ident);

        quote! {
          match self {
            #name::#label { .. } => {
              let tag = ::yaserde::xml::XmlName::local(#tag);
              child_attributes.push(::yaserde::xml::XmlAttribute::new(tag, #element_name));
            }
            _ => {}
          }
        }
      } else { quote!() };

      let all_fields = variant
        .fields
        .iter()
        .map(|field| YaSerdeField::new(field.clone()));

      let attribute_fields: Vec<_> = all_fields
        .clone()
        .filter(|field| {
          field.is_attribute()
            || (field.is_flatten() && matches!(field.get_type(), Field::FieldStruct { .. }))
        })
        .collect();

      let add_attributes : TokenStream = attribute_fields
        .iter()
        .map(|field| {
          let label = variant.ident.clone();
          let var = get_id(field);
          let name = name.clone();

          let destructure = if field.get_value_label().is_some() {
            quote! {{#var, ..}}
          } else {
            quote! {(#var, ..)}
          };

          if field.is_attribute() {
            quote! { #name::#label { .. } => { }, }
          } else {
            match field.get_type() {
              Field::FieldStruct { .. } => {
                if root_attributes.flatten {
                  quote! {
                    match self {
                      #name::#label #destructure => {
                        let (serialized_attributes, serialized_namespace) = #var.serialize_attributes(
                          ::std::vec::Vec::<::yaserde::xml::XmlAttribute>::new(),
                          ::yaserde::xml::XmlNamespace::empty(),
                        )?;
                        child_attributes_namespace.extend(&serialized_namespace);
                        child_attributes.extend(serialized_attributes);
                      },
                      _ => {}
                    }
                  }
                } else {
                  quote! {}
                }
              }
              _ => quote! { #name::#label { .. } => { },},
            }
          }
        })
        .collect();

        quote!( #add_attributes #add_tag)
    })
    .collect();

  implement_serializer(
    name,
    root,
    root_attributes,
    quote!(#variant_matches),
    quote!(match self {
      #inner_enum_inspector
    }),
    generics,
  )
}

fn inner_enum_inspector(
  data_enum: &DataEnum,
  name: &Ident,
  root_attributes: &YaSerdeAttribute,
) -> TokenStream {
  data_enum
    .variants
    .iter()
    .map(|variant| {
      let variant_attrs = YaSerdeAttribute::from(&variant.attrs);

      let label = &variant.ident;
      let label_name = build_label_name(label, &variant_attrs, &root_attributes.default_namespace);

      match variant.fields {
        Fields::Unit => {
          if let Some(_tag) = &root_attributes.tag {
            quote! { #name::#label => {} }
          } else {
            quote! {
              #name::#label => {
                writer.write_characters(#label_name)?;
              }
            }
          }
        }
        Fields::Named(ref fields) => {
          let enum_fields: TokenStream = fields
            .named
            .iter()
            .map(|field| YaSerdeField::new(field.clone()))
            .filter(|field| !field.is_attribute())
            .filter_map(|field| {
              let field_label = field.label();

              if field.is_text_content() {
                return Some(quote!(
                  writer.write_characters(&self.#field_label)?;
                ));
              }

              let field_label_name = field.renamed_label(root_attributes);

              match field.get_type() {
                Field::FieldString
                | Field::FieldBool
                | Field::FieldU8
                | Field::FieldI8
                | Field::FieldU16
                | Field::FieldI16
                | Field::FieldU32
                | Field::FieldI32
                | Field::FieldF32
                | Field::FieldU64
                | Field::FieldI64
                | Field::FieldF64 => Some({
                  quote! {
                    match self {
                      &#name::#label { ref #field_label, .. } => {
                        writer.write_start_element(
                          #field_label_name,
                          ::std::vec![],
                          ::yaserde::xml::XmlNamespace::empty(),
                        )?;

                        let string_value = #field_label.to_string();
                        writer.write_characters(&string_value)?;

                        writer.write_end_element()?;
                      },
                      _ => {},
                    }
                  }
                }),
                Field::FieldStruct { .. } => Some(quote! {
                  match self {
                    &#name::#label{ref #field_label, ..} => {
                      writer.set_start_event_name(
                        ::std::option::Option::Some(#field_label_name.to_string()),
                      );
                      writer.set_skip_start_end(false);
                      ::yaserde::YaSerialize::serialize(#field_label, writer)?;
                    },
                    _ => {}
                  }
                }),
                Field::FieldVec { .. } => Some(quote! {
                  match self {
                    &#name::#label { ref #field_label, .. } => {
                      for item in #field_label {
                        writer.set_start_event_name(
                          ::std::option::Option::Some(#field_label_name.to_string()),
                        );
                        writer.set_skip_start_end(false);
                        ::yaserde::YaSerialize::serialize(item, writer)?;
                      }
                    },
                    _ => {}
                  }
                }),
                Field::FieldOption { .. } => None,
              }
            })
            .collect();

          quote! {
            &#name::#label{..} => {
              #enum_fields
            }
          }
        }
        Fields::Unnamed(ref fields) => {
          let enum_fields: TokenStream = fields
            .unnamed
            .iter()
            .map(|field| YaSerdeField::new(field.clone()))
            .filter(|field| !field.is_attribute())
            .map(|field| {
              let write_element = |action: &TokenStream| {
                quote! {
                  writer.write_start_element(
                    #label_name,
                    ::std::vec![],
                    ::yaserde::xml::XmlNamespace::empty(),
                  )?;

                  #action

                  writer.write_end_element()?;
                }
              };

              let write_string_chars = quote! {
                writer.write_characters(item)?;
              };

              let write_simple_type = write_element(&quote! {
                let s = item.to_string();
                writer.write_characters(&s)?;
              });

              let serialize = quote! {
                writer.set_start_event_name(::std::option::Option::None);
                writer.set_skip_start_end(true);
                ::yaserde::YaSerialize::serialize(item, writer)?;
              };

              let write_sub_type = |data_type| {
                write_element(match data_type {
                  Field::FieldString => &write_string_chars,
                  _ => &serialize,
                })
              };

              let match_field = |write: &TokenStream| {
                quote! {
                  match self {
                    &#name::#label(ref item) => {
                      #write
                    },
                    _ => {},
                  }
                }
              };

              match field.get_type() {
                Field::FieldOption { data_type } => {
                  let write = write_sub_type(*data_type);

                  match_field(&quote! {
                    if let ::std::option::Option::Some(item) = item {
                      #write
                    }
                  })
                }
                Field::FieldVec { data_type } => {
                  let write = write_sub_type(*data_type);

                  match_field(&quote! {
                    for item in item {
                      #write
                    }
                  })
                }
                Field::FieldStruct { .. } => {
                  if variant_attrs.flatten || field.is_flatten() {
                    match_field(&quote! { ::yaserde::YaSerialize::serialize(item, writer)?})
                  } else {
                    write_element(&match_field(&serialize))
                  }
                }
                Field::FieldString => match_field(&write_element(&write_string_chars)),
                _simple_type => match_field(&write_simple_type),
              }
            })
            .collect();

          quote! {
            &#name::#label{..} => {
              #enum_fields
            }
          }
        }
      }
    })
    .collect()
}
