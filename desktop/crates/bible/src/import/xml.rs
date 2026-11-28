use quick_xml::events::BytesStart;

pub(super) fn local(element: &BytesStart) -> String {
    element.local_name().as_ref().to_string()
}

pub(super) fn attribute(element: &BytesStart, name: &str) -> Option<String> {
    let value = element.try_get_attribute(name).ok().flatten()?;
    value.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|v| v.into_owned())
}
