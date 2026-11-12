// C++23 with GCC 13's library, and OpenMP: what OpenMS needs of the compiler
#include <expected>
#include <format>
#include <iostream>
#include <numeric>
#include <omp.h>
#include <vector>

std::expected<double, std::string> mean(const std::vector<double>& v)
{
  if (v.empty()) return std::unexpected("empty");
  double sum = 0;
#pragma omp parallel for reduction(+ : sum)
  for (std::size_t i = 0; i < v.size(); ++i) sum += v[i];
  return sum / static_cast<double>(v.size());
}

int main()
{
  std::vector<double> v(1000);
  std::iota(v.begin(), v.end(), 1.0);
  const auto m = mean(v);
  std::cout << std::format("mean {:.1f}, {} threads, GCC {}.{}\n", m.value(), omp_get_max_threads(), __GNUC__, __GNUC_MINOR__);
  return m.value() == 500.5 && !mean({}).has_value() ? 0 : 1;
}
