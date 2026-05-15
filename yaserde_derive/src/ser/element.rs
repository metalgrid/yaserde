use crate::common::YaSerdeField;
use proc_macro2::{Ident, TokenStream};
use quote::quote;

pub fn enclose_formatted_characters(label: &Ident, label_name: String) -> TokenStream {
  enclose_xml_event(label_name, quote!(format!("{}", &self.#label)))
}

pub fn enclose_formatted_characters_for_value(label: &Ident, label_name: String) -> TokenStream {
  enclose_xml_event(label_name, quote!(format!("{}", #label)))
}

pub fn enclose_characters(label: &Option<Ident>, label_name: String) -> TokenStream {
  enclose_xml_event(label_name, quote!(format!("{}", self.#label)))
}

fn enclose_xml_event(label_name: String, yaserde_format: TokenStream) -> TokenStream {
  quote! {
    writer.write_start_element(#label_name, ::std::iter::empty::<(::std::string::String, ::std::string::String)>())?;

    let yaserde_value = #yaserde_format;
    writer.write_text(&yaserde_value)?;

    writer.write_end_element(#label_name)?;
  }
}

pub fn serialize_element(
  label: &Option<Ident>,
  label_name: String,
  conditions: &TokenStream,
) -> Option<TokenStream> {
  let inner = enclose_characters(label, label_name);

  Some(quote! {
    #conditions {
      #inner
    }
  })
}

pub fn condition_generator(label: &Option<Ident>, field: &YaSerdeField) -> TokenStream {
  let default_condition = field
    .get_default_function()
    .map(|default_function| quote!(self.#label != #default_function()));

  let skip_serializing = field.is_skip_serializing();

  field
    .get_skip_serializing_if_function()
    .map(|skip_if_function| {
      if let Some(prev_conditions) = &default_condition {
        quote!(if !#skip_serializing && !self.#skip_if_function(&self.#label) && #prev_conditions)
      } else {
        quote!(if !#skip_serializing && !self.#skip_if_function(&self.#label))
      }
    })
    .unwrap_or_else(|| {
      default_condition
        .map(|condition| quote!(if !#skip_serializing && #condition))
        .unwrap_or(quote!(if !#skip_serializing))
    })
}
