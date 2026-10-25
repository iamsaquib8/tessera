import os


def account_detail(request, account_id):
    return os.path.join("accounts", str(account_id))


def account_list(request):
    return []
