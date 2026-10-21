using Acme.Util;

namespace Acme.Users
{
    public class UserService
    {
        private readonly UserRepository _repo = new UserRepository();

        public string Load(string id)
        {
            return Strings.Trim(_repo.FindById(id));
        }
    }
}
