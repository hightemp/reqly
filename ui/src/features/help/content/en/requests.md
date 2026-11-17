# Requests

## URL and parameters

The URL bar and the **Params** tab stay in sync: query parameters typed in
the URL appear in the table and back. Path parameters are written as
`:name` (`/users/:id`) and get their values in the table. Every field
accepts [variables](help:variables): `{{ base_url }}/users`.

## Headers

Headers of the request are added to the headers of its folders. A row can be
switched off without deleting it. The `Content-Type` of a body is set
automatically unless you set your own.

## Body

| Type                        | Use                                                    |
| --------------------------- | ------------------------------------------------------ |
| JSON, XML, YAML, Text, HTML | text with highlighting; `Ctrl+Shift+B` formats it      |
| Form URL-encoded            | key / value pairs                                      |
| Multipart                   | fields and files                                       |
| Binary                      | a file sent as it is                                   |
| GraphQL                     | a query and variables, see [Protocols](help:protocols) |

## Auth

The **Auth** tab: Basic, Bearer, API key, Digest, NTLM, OAuth 1.0 and 2.0,
AWS Signature v4 and more. Requests inherit the auth of their folder. See
[Authentication](help:auth).

## Settings

The **Settings** tab of a request (and of a folder) overrides the network
settings: timeouts, redirects, TLS verification, HTTP version, proxy,
compression, DNS overrides. Global defaults are in **Settings → Requests &
proxy**.

## The response

- **Body**: pretty (JSON, XML, HTML are formatted), raw, or a preview
  (HTML, images, PDF, audio and video). Filter JSON with JSONPath (`$.items[0]`)
  or jq, XML with XPath.
- **Headers**, **Cookies** and the **Timeline**: the raw exchange with
  timings (DNS, connect, TLS, first byte).
- **Tests** and **Console**: results of [assertions](help:testing) and script
  output.
- **Response history** keeps earlier responses of the request; pick one to
  look at it again.

The **Request log** (command palette) lists everything sent recently.
**Cookies** (command palette) manages the cookies of the workspace.

## Code and cURL

**Generate code…** turns a request into code for 26 languages and libraries
(cURL, fetch, Axios, Python Requests, Go, Java, C# and more). **Copy as** in
the context menu copies it right away. Pasting a curl command into the URL
bar does the opposite.

## Documentation

The **Docs** tab of a request or folder holds a Markdown description.
