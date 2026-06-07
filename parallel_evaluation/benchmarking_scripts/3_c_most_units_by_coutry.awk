{ u[$2] += $9 } END { for (i in u) if (u[i] > u_max) { u_max = u[i]; c = i }  print c, u_max }
