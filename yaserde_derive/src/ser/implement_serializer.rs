use crate::common::YaSerdeAttribute;
use crate::ser::namespace::generate_namespaces_definition;
use proc_macro2::Ident;
use proc_macro2::TokenStream;
use quote::quote;
use syn::Generics;

pub fn implement_serializer(
  name: &Ident,
  root: &str,
  attributes: &YaSerdeAttribute,
  append_attributes: TokenStream,
  inner_inspector: TokenStream,
  generics: &Generics,
) -> TokenStream {
  let namespaces_definition = generate_namespaces_definition(attributes);
  let flatten = attributes.flatten;

  let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

  quote! {
    impl #impl_generics ::yaserde::YaSerialize for #name #ty_generics #where_clause {
      #[allow(unused_variables)]
      fn serialize<W: ::std::io::Write>(
        &self,
        writer: &mut ::yaserde::ser::Serializer<W>,
      ) -> ::std::result::Result<(), ::std::string::String> {
        let skip = writer.skip_start_end();

        if !#flatten && !skip {
          let mut attributes = ::std::vec::Vec::<::yaserde::xml::XmlAttribute>::new();
          let mut namespace = ::yaserde::xml::XmlNamespace::empty();
          let mut child_attributes = ::std::vec::Vec::<::yaserde::xml::XmlAttribute>::new();
          let mut child_attributes_namespace = ::yaserde::xml::XmlNamespace::empty();

          let yaserde_label = writer.get_start_event_name().unwrap_or_else(|| #root.to_string());
          #namespaces_definition
          #append_attributes

          attributes.extend(child_attributes);
          namespace.extend(&child_attributes_namespace);

          writer.write_start_element(yaserde_label, attributes, namespace)?;
        }

        #inner_inspector

        if !#flatten && !skip {
          writer.write_end_element()?;
        }

        ::std::result::Result::Ok(())
      }

      fn serialize_attributes(
        &self,
        mut source_attributes: ::std::vec::Vec<::yaserde::xml::XmlAttribute>,
        mut source_namespace: ::yaserde::xml::XmlNamespace,
      ) -> ::std::result::Result<
        (::std::vec::Vec<::yaserde::xml::XmlAttribute>, ::yaserde::xml::XmlNamespace),
        ::std::string::String
      > {
        let mut attributes = ::std::vec::Vec::<::yaserde::xml::XmlAttribute>::new();
        let mut namespace = ::yaserde::xml::XmlNamespace::empty();
        let mut child_attributes = ::std::vec::Vec::<::yaserde::xml::XmlAttribute>::new();
        let mut child_attributes_namespace = ::yaserde::xml::XmlNamespace::empty();

        #namespaces_definition
        #append_attributes

        source_namespace.extend(&namespace);
        source_namespace.extend(&child_attributes_namespace);
        source_attributes.extend(attributes);
        source_attributes.extend(child_attributes);

        ::std::result::Result::Ok((source_attributes, source_namespace))
      }
    }
  }
}
