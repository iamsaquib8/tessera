using System;
using System.Collections.Generic;

namespace Sample
{
    public class UserService
    {
        public string FindById(int id)
        {
            return Render(LoadUser(id));
        }

        private string LoadUser(int id)
        {
            return $"user#{id}";
        }

        private string Render(string user)
        {
            return $"[{user}]";
        }

        public string RenderUser(int id)
        {
            return FindById(id);
        }
    }
}
