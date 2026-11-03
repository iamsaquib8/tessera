; Sinatra routes: get, post, put, patch, delete, head, options with a path string and block.
; Docs: sinatrarb.com/intro.html#routes

(call
  !receiver
  method: (identifier) @method
  arguments: (argument_list . (string) @path)
  block: [(do_block) (block)]
  (#match? @method "^(get|post|put|patch|delete|head|options)$")) @route
