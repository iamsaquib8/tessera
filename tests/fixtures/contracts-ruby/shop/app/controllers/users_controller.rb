class UsersController < ApplicationController
  def show
    @user = User.find(params[:id])
    Rails.cache.fetch("/users/#{params[:id]}") { @user }
  end

  def create
    settings.get("/api/status")
    head :created
  end
end
