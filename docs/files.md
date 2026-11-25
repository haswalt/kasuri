# Workflow file references

An Email Trigger can keep supported attachments in private Cloudflare R2 storage. Workflow input and run history carry only file references, with an opaque ID, filename, content type, and byte size. They contain neither attachment bytes nor a public link.

Enable the optional `FILES` R2 binding in `wrangler.jsonc` and use a private bucket dedicated to the installation. [Set an R2 lifecycle rule](https://developers.cloudflare.com/r2/buckets/object-lifecycles/) that retains attachments for at least as long as workflows may wait before using them. An expired reference follows the node's error output. Workflows and run-history retention do not delete bucket objects.

Email Trigger exposes `attachments` and `attachmentsSkipped`. Skip counts include files that were unsupported, exceeded the limits or arrived with no bucket enabled. Route a nonzero skip count to a person if the workflow needs all attachments. Kasuri stores up to four matching PDF, PNG, JPEG or WebP attachments per message, up to 4 MiB each and 8 MiB combined. Clairic additionally enforces its rendered-page limit. The file signature determines the content type.

In a **Clairic** node following Email Trigger, select `/attachments` under **Attachments** and `/body` under **Text to read**. With a nonempty file list, text is optional. Clairic resolves references only in the active workspace and checks the stored size and media type before sending bytes to Workers AI. A reference from another workspace, a URL, or an expired file cannot be resolved. Simulations validate the metadata and use fixture answers without reading R2 or calling the model.

Try the [offline attachment example](../examples/workflows/35-clairic-booking-attachment.json) to see a PDF reference produce typed booking details. It uses synthetic metadata and contains no file bytes.
