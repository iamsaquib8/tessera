; Rails routes: verbs, root and resources in config/routes.rb's draw block,
; and one level of namespace or scope. `@resources` draws the REST routes
; (only:/except: honoured). Docs: guides.rubyonrails.org/routing.html
; Only calls directly inside `routes.draw do … end` count.

; get "/users/:id", to: "users#show"
(call
  method: (identifier) @draw
  block: (do_block
    body: (body_statement
      (call
        method: (identifier) @method
        arguments: (argument_list . (string) @path)) @route))
  (#eq? @draw "draw")
  (#match? @method "^(get|post|put|patch|delete)$"))

; root to: "home#index"
(call
  method: (identifier) @draw
  block: (do_block
    body: (body_statement
      (call method: (identifier) @root) @route))
  (#eq? @draw "draw")
  (#eq? @root "root")
  (#set! method "GET"))

; resources :orders, only: [:index, :show]
(call
  method: (identifier) @draw
  block: (do_block
    body: (body_statement
      (call
        method: (identifier) @resources
        arguments: (argument_list . (simple_symbol) @path) @resource_args) @route))
  (#eq? @draw "draw")
  (#match? @resources "^resources?$"))

; namespace :api do get "/status" end, scope "/admin" do … end
(call
  method: (identifier) @draw
  block: (do_block
    body: (body_statement
      (call
        method: (identifier) @scope
        arguments: (argument_list . [(simple_symbol) (string)] @prefix)
        block: (do_block
          body: (body_statement
            (call
              method: (identifier) @method
              arguments: (argument_list . (string) @path)) @route)))))
  (#eq? @draw "draw")
  (#match? @scope "^(namespace|scope)$")
  (#match? @method "^(get|post|put|patch|delete)$"))

(call
  method: (identifier) @draw
  block: (do_block
    body: (body_statement
      (call
        method: (identifier) @scope
        arguments: (argument_list . [(simple_symbol) (string)] @prefix)
        block: (do_block
          body: (body_statement
            (call
              method: (identifier) @resources
              arguments: (argument_list . (simple_symbol) @path) @resource_args) @route)))))
  (#eq? @draw "draw")
  (#match? @scope "^(namespace|scope)$")
  (#match? @resources "^resources?$"))
