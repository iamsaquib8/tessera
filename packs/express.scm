; Express, Fastify, Koa-router and Hono style route registration.
; Docs: expressjs.com/en/guide/routing.html, fastify.dev/docs/latest/Reference/Routes/

; app.get("/users/:id", handler), router.post(...), this.app.put(...)
; A handler argument after the path tells a route from map.get("k", fallback).
(call_expression
  function: (member_expression
    object: [(identifier) @object
             (member_expression property: (property_identifier) @object)]
    property: (property_identifier) @method)
  arguments: (arguments . [(string) (template_string)] @path . (_))
  (#match? @object "^(app|router|server|routes?|[a-zA-Z]*Router|fastify|App)$")
  (#match? @method "^(get|post|put|patch|delete|head|options|all)$")) @route

; router.route("/users").get(handler)
(call_expression
  function: (member_expression
    object: (call_expression
      function: (member_expression property: (property_identifier) @route_fn)
      arguments: (arguments . [(string) (template_string)] @path))
    property: (property_identifier) @method)
  (#eq? @route_fn "route")
  (#match? @method "^(get|post|put|patch|delete|head|options|all)$")) @route

; fastify.route({ method: "GET", url: "/x", handler })
(call_expression
  function: (member_expression property: (property_identifier) @route_fn)
  arguments: (arguments . (object
    (pair key: (property_identifier) @method_key value: (string) @method)
    (pair key: (property_identifier) @url_key value: [(string) (template_string)] @path)))
  (#eq? @route_fn "route")
  (#eq? @method_key "method")
  (#eq? @url_key "url")) @route

(call_expression
  function: (member_expression property: (property_identifier) @route_fn)
  arguments: (arguments . (object
    (pair key: (property_identifier) @url_key value: [(string) (template_string)] @path)
    (pair key: (property_identifier) @method_key value: (string) @method)))
  (#eq? @route_fn "route")
  (#eq? @method_key "method")
  (#eq? @url_key "url")) @route

; app.use("/api", users): the routes on `users` answer under /api. Joined to
; the routes at query time, through the file's import of `users`.
(call_expression
  function: (member_expression
    property: (property_identifier) @use_fn)
  arguments: (arguments . [(string) (template_string)] @path . (identifier) @handler .)
  (#eq? @use_fn "use")) @mount
