//! Discovery schemas shared by management tools and future connector forms.
use serde_json::{Value, json};
pub fn query_schema() -> Value {
    json!({"type":"object","properties":{"cursor":{"type":"string","maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100,"default":25},"filters":{"type":"array","maxItems":16,"items":{"type":"object","properties":{"pointer":{"type":"string","maxLength":256,"description":"JSON Pointer into the record value; empty string selects the whole value."},"operator":{"enum":["eq","ne","gt","lt","exists"]},"value":{}},"required":["pointer","operator"],"additionalProperties":false}}},"additionalProperties":false})
}
pub fn command_schema() -> Value {
    let id = json!({"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z0-9_-]+$"});
    let key = json!({"type":"string","minLength":1,"maxLength":512});
    let version = json!({"type":"integer","minimum":0});
    let ttl = json!({"type":"integer","minimum":1,"description":"Positive milliseconds until expiry; omit for no expiry on insert/put."});
    let definition = json!({"type":"object","properties":{"id":id,"name":{"type":"string","minLength":1,"maxLength":160},"kind":{"enum":["table","context"]},"fields":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"name":id,"kind":{"enum":["string","number","boolean","object","array","any"]},"required":{"type":"boolean","default":false}},"required":["name","kind"],"additionalProperties":false}}},"required":["id","name","kind"],"additionalProperties":false});
    let variant = |operation: &str, mut properties: Value, mut required: Vec<&str>| {
        properties["operation"] = json!({"const":operation});
        required.push("operation");
        json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
    };
    json!({"oneOf":[
        variant("create_table",json!({"definition":definition}),vec!["definition"]),
        variant("rename_table",json!({"tableId":id,"name":{"type":"string","minLength":1,"maxLength":160},"expectedVersion":version}),vec!["tableId","name","expectedVersion"]),
        variant("get_table",json!({"tableId":id}),vec!["tableId"]),
        variant("get",json!({"tableId":id,"key":key}),vec!["tableId","key"]),
        variant("insert_if_absent",json!({"tableId":id,"key":key,"value":{},"ttlMs":ttl}),vec!["tableId","key","value"]),
        variant("put",json!({"tableId":id,"key":key,"value":{},"expectedVersion":version,"ttlMs":ttl}),vec!["tableId","key","value"]),
        variant("patch",json!({"tableId":id,"key":key,"patch":{"type":"object","description":"Replace supplied top-level fields while preserving unmentioned fields and expiry."},"removeFields":{"type":"array","maxItems":100,"items":{"type":"string"}},"expectedVersion":version}),vec!["tableId","key","patch"]),
        variant("delete",json!({"tableId":id,"key":key,"expectedVersion":version}),vec!["tableId","key"]),
        variant("query",json!({"tableId":id,"query":query_schema()}),vec!["tableId"])
    ]})
}
pub fn mutation_schema() -> Value {
    let mut schema = command_schema();
    schema["oneOf"].as_array_mut().unwrap().retain(|variant| {
        !matches!(
            variant["properties"]["operation"]["const"].as_str(),
            Some("get" | "get_table" | "query")
        )
    });
    schema
}
