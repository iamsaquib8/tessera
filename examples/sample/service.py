def normalize_user_name(name):
    return name.strip().title()


def display_user(user):
    return normalize_user_name(user["name"])
