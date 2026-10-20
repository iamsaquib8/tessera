<?php

namespace App\Users;

use App\Util\Strings;

class UserService
{
    private $repo;

    public function __construct()
    {
        $this->repo = new UserRepository();
    }

    public function load($id)
    {
        return Strings::trim($this->repo->findById($id));
    }
}
