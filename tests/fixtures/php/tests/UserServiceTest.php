<?php

use App\Users\UserService;

class UserServiceTest extends TestCase
{
    public function testLoad()
    {
        (new UserService())->load(1);
    }
}
