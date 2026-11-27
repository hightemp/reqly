# Variables and environments

## Using variables

Write `{{ name }}` in the URL, parameters, headers, body, auth fields or in
other variables. Variables are highlighted, undefined ones in red. Hover one to see its value; the editor completes names as you type,
and right after `{{` it also lists functions such as `$uuid` and `$timestamp`.

Paths into JSON values work too: `{{ api.version }}`, `{{ hosts[0] }}`.

## Where variables come from

Later levels override earlier ones:

1. the active **global environment** of the workspace;
2. the collection's **base environment**;
3. the collection's active **environment** (dev, stage, prod…);
4. **folder variables**, from the outermost folder inwards;
5. **request variables** (the **Variables** tab);
6. variables set by [scripts](help:scripts) while the app runs.

Edit environments in the **Environments** window (`Ctrl+E`) as a table or as
JSON. Switch the active environment in the sidebar header.

## Secrets

Mark a key as secret with the lock in its row. The value is masked and
stored encrypted. Exports leave secret values empty unless you choose to
include them.

## Template tags

Functions generate values when a request is sent:

| Tag                                             | Gives                                   |
| ----------------------------------------------- | --------------------------------------- |
| `{{ $uuid }}`, `{% uuid 'v7' %}`                | a UUID                                  |
| `{% now 'iso-8601' %}`                          | the current time                        |
| `{% random 'int', 1, 100 %}`                    | a random number or string               |
| `{% base64 'encode', 'normal', 'user:pass' %}`  | Base64                                  |
| `{% hash 'sha256', 'hex', body %}`              | a hash or HMAC                          |
| `{% jwt 'encode', '{"sub":"1"}', secret %}`     | a signed JWT                            |
| `{% prompt 'OTP', 'Code from the SMS' %}`       | asks you before sending                 |
| `{% response 'body', '<request>', '$.token' %}` | a value from another request's response |

Hover a tag and click **Edit…** (or `Ctrl`+click) to change its arguments
with a live preview. To add a new tag, right-click in a field or editor and
choose **Insert template tag…**: pick the function, then fill in each
argument (lists, numbers, a request from the tree).

## Response chaining

`{% response %}` takes a value from the latest response of another request:
the body (JSONPath `$.token` or XPath), a header, the status or the URL. It
can send that request first: when there is no response yet, when the
response is too old, or every time.

### Example: a token from the login response

1. Send the login request once, so it has a response with the token, e.g.
   `{"token": "eyJ…"}`.
2. Open the request that needs the token, **Auth → Bearer token**.
3. Right-click the **Token** field and choose **Insert template tag…**.
4. **Function**: Response. **Attribute**: Body. **Request**: type a part of
   the login request's name and pick it. **Filter**: `$.token`.
5. **Trigger behavior**: Never uses the latest response; When expired sends
   the login request again when its response is older than **Max age**.
6. The preview shows the token. **Apply** inserts
   `{% response 'body', '<request id>', '$.token' %}`.

You do not need to know the request ID: the editor fills it in. **Copy path /
ID → Copy ID** in the request's context menu copies it if you write the tag
by hand. Filters imported from Insomnia (`b64::…::46b`) work as they are and
show as plain text in the editor.

## Fake data

`{{ $faker.person.firstName }}`, `{{ $faker.internet.email }}`,
`{{ $faker.number.int({ min: 1, max: 100 }) }}` — every faker-js function in
every locale. The locale is set in **Settings → Templates**. Postman dynamic
variables (`{{ $guid }}`, `{{ $randomEmail }}`, `{{ $timestamp }}`…) work
too.
