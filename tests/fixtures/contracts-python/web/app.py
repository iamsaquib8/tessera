from flask import Blueprint, Flask

app = Flask(__name__)
items = Blueprint("items", __name__, url_prefix="/items")


@app.route("/carts/<int:cart_id>", methods=["POST"])
def update_cart(cart_id):
    return str(cart_id)


@app.route("/carts")
def list_carts():
    return "[]"


@items.route("/<item_id>")
def get_item(item_id):
    return item_id


@items.delete("/<item_id>")
def delete_item(item_id):
    return ""


app.register_blueprint(items)
