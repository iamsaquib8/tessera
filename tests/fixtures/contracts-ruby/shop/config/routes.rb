Rails.application.routes.draw do
  root to: "home#index"
  get "/users/:id", to: "users#show"
  post "users", to: "users#create"
  resources :orders
  resources :reviews, only: [:index, :show]

  namespace :api do
    get "/status", to: "status#show"
    resources :carts, except: [:new, :edit]
  end
end
